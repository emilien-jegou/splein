// Provides client IPC communication utilities over a local Unix domain socket.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

pub fn socket_path() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(|dir| PathBuf::from(dir).join("splein.sock"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/splein.sock"))
}

pub struct IpcClient;

impl IpcClient {
    pub fn send_command(cmd: &str) -> eyre::Result<()> {
        let path = socket_path();
        let mut stream = UnixStream::connect(&path)?;
        stream.write_all(cmd.as_bytes())?;
        Ok(())
    }
}
