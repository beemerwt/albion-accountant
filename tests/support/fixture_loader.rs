use std::{fs, path::PathBuf};

pub fn load_hex_fixture(name: &str) -> Vec<u8> {
    let path = fixture_path(name);
    let raw = fs::read_to_string(path).expect("hex fixture readable");
    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.len() % 2 == 0, "hex fixture must have even length");

    compact
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("utf8 hex"), 16).expect("hex byte")
        })
        .collect()
}

pub fn load_pcapng_packets(name: &str) -> Vec<Vec<u8>> {
    let path = fixture_path(name);
    let bytes = fs::read(path).expect("pcapng fixture readable");
    let mut packets = Vec::new();
    let mut offset = 0usize;

    while offset + 12 <= bytes.len() {
        let block_type = le_u32(&bytes[offset..offset + 4]);
        let block_len = le_u32(&bytes[offset + 4..offset + 8]) as usize;
        if block_len < 12 || offset + block_len > bytes.len() {
            break;
        }
        let block_end = offset + block_len;
        let trailing_len = le_u32(&bytes[block_end - 4..block_end]) as usize;
        if trailing_len != block_len {
            break;
        }

        // Enhanced Packet Block
        if block_type == 0x00000006 && block_len >= 32 {
            let captured_len = le_u32(&bytes[offset + 20..offset + 24]) as usize;
            let data_start = offset + 28;
            let available = block_end.saturating_sub(4).saturating_sub(data_start);
            let packet_len = captured_len.min(available);
            packets.push(bytes[data_start..data_start + packet_len].to_vec());
        }

        offset = block_end;
    }

    packets
}

fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}
