use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};
use ferrum_core::types::{IpcCommand, IpcResponse};

const SOCK_PATH: &str = "/tmp/ferrum.sock";

/// Open a fresh Unix socket connection, send one command, return the response.
/// Each HTTP request gets its own connection — the daemon handles concurrent clients fine.
pub async fn send_ipc(cmd: IpcCommand) -> Option<IpcResponse> {
    let mut stream = UnixStream::connect(SOCK_PATH).await.ok()?;

    let mut msg = serde_json::to_string(&cmd).ok()?;
    msg.push('\n');
    stream.write_all(msg.as_bytes()).await.ok()?;

    let mut line = String::new();
    let mut reader = BufReader::new(&mut stream);
    reader.read_line(&mut line).await.ok()?;

    serde_json::from_str::<IpcResponse>(line.trim()).ok()
}
