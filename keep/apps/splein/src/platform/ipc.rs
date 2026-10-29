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
    /// Sends a command, returning false when no daemon is listening on the socket.
    pub fn try_send_command(cmd: &str) -> eyre::Result<bool> {
        let path = socket_path();
        let Ok(mut stream) = UnixStream::connect(&path) else {
            return Ok(false);
        };
        stream.write_all(cmd.as_bytes())?;
        Ok(true)
    }

    /// Sends a command, failing when no daemon is listening on the socket.
    pub fn send_command(cmd: &str) -> eyre::Result<()> {
        if !Self::try_send_command(cmd)? {
            eyre::bail!("no splein daemon is running; start one with `splein daemon`");
        }
        Ok(())
    }
}
