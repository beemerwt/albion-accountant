use crate::error::Result;
use albion_network_lib::DecodedPacket;

#[cfg(target_os = "linux")]
use albion_network_lib::{HostFilter, PhotonParser, extract_udp_payload};

#[cfg(target_os = "linux")]
use std::{ffi::CString, fs, mem, os::fd::RawFd, path::Path};

#[cfg(target_os = "linux")]
const ETH_P_ALL: u16 = 0x0003;

#[cfg(target_os = "linux")]
pub fn process_live_capture(
    debug: bool,
    mut on_packet: impl FnMut(DecodedPacket) -> Result<()>,
) -> Result<()> {
    eprintln!("INFO:albion:starting live capture on all available interfaces");
    let host_filter = HostFilter::from_file(Path::new("hosts.txt"))?;
    eprintln!(
        "INFO:albion:loaded {} allowed host ranges from hosts.txt",
        host_filter.len()
    );

    let sockets = open_capture_sockets()?;
    if sockets.is_empty() {
        return Err("No capture interfaces could be opened".into());
    }

    let names = sockets
        .iter()
        .map(|socket| socket.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("INFO:albion:capturing on {names}");

    let mut polls = sockets
        .iter()
        .map(|socket| libc::pollfd {
            fd: socket.fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect::<Vec<_>>();
    let mut parser = PhotonParser::new("live".to_string(), debug);
    let mut emitted_packets = 0usize;
    let mut packet_number = 0usize;
    let mut frame = vec![0u8; 65536];

    loop {
        let ready = unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, -1) };
        if ready < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        for index in 0..polls.len() {
            if polls[index].revents & libc::POLLIN == 0 {
                continue;
            }

            let length =
                unsafe { libc::recv(sockets[index].fd, frame.as_mut_ptr().cast(), frame.len(), 0) };
            if length < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() != std::io::ErrorKind::WouldBlock {
                    eprintln!(
                        "WARN:albion:{} receive failed: {}",
                        sockets[index].name, error
                    );
                }
                continue;
            }

            packet_number += 1;
            let Some(packet) = extract_udp_payload(&frame[..length as usize], Some(1)) else {
                continue;
            };
            if !(packet.source.is_albion_port() || packet.destination.is_albion_port()) {
                continue;
            }
            if !(host_filter.contains(packet.source.ip)
                || host_filter.contains(packet.destination.ip))
            {
                continue;
            }

            parser.receive_packet(
                packet.payload,
                packet_number,
                &packet.source.to_string(),
                &packet.destination.to_string(),
            )?;

            for decoded in &parser.decoded_packets()[emitted_packets..] {
                on_packet(decoded.clone())?;
            }
            emitted_packets = parser.decoded_packets().len();
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn process_live_capture(
    _debug: bool,
    _on_packet: impl FnMut(DecodedPacket) -> Result<()>,
) -> Result<()> {
    Err("Live capture is only supported on Linux".into())
}

#[cfg(target_os = "linux")]
fn open_capture_sockets() -> Result<Vec<CaptureSocket>> {
    let mut sockets = Vec::new();
    for entry in fs::read_dir("/sys/class/net")? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        match CaptureSocket::open(&name) {
            Ok(socket) => sockets.push(socket),
            Err(error) => eprintln!("WARN:albion:{name} capture disabled: {}", error.0),
        }
    }
    Ok(sockets)
}

#[cfg(target_os = "linux")]
struct CaptureSocket {
    fd: RawFd,
    name: String,
}

#[cfg(target_os = "linux")]
impl CaptureSocket {
    fn open(name: &str) -> Result<Self> {
        let interface =
            CString::new(name).map_err(|_| format!("Invalid interface name {name:?}"))?;
        let ifindex = unsafe { libc::if_nametoindex(interface.as_ptr()) };
        if ifindex == 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let fd = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                i32::from(ETH_P_ALL.to_be()),
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let address = libc::sockaddr_ll {
            sll_family: libc::AF_PACKET as u16,
            sll_protocol: ETH_P_ALL.to_be(),
            sll_ifindex: ifindex as i32,
            sll_hatype: 0,
            sll_pkttype: 0,
            sll_halen: 0,
            sll_addr: [0; 8],
        };

        let bind_result = unsafe {
            libc::bind(
                fd,
                (&address as *const libc::sockaddr_ll).cast(),
                mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
            )
        };
        if bind_result < 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(error.into());
        }

        Ok(Self {
            fd,
            name: name.to_string(),
        })
    }
}

#[cfg(target_os = "linux")]
impl Drop for CaptureSocket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}
