use std::process::Command;

pub fn open_url_in_browser(url: &str) -> std::io::Result<()> {
    browser_open_command(url).spawn().map(|_| ())
}

#[cfg(target_os = "windows")]
fn browser_open_command(url: &str) -> Command {
    let mut command = Command::new("cmd");
    command.args(["/C", "start", "", url]);
    command
}

#[cfg(target_os = "macos")]
fn browser_open_command(url: &str) -> Command {
    let mut command = Command::new("open");
    command.arg(url);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn browser_open_command(url: &str) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    command
}
