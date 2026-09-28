// crates/splein/src/platform/ipc.rs

use std::io::Write;
use std::os::unix::net::UnixStream;

pub const SOCKET_PATH: &str = "/tmp/presentify.sock";

pub struct IpcClient;

impl IpcClient {
    pub fn send_command(cmd: &str) -> eyre::Result<()> {
        let mut stream = UnixStream::connect(SOCKET_PATH)?;
        stream.write_all(cmd.as_bytes())?;
        Ok(())
    }
}
