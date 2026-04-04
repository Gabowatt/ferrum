use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};
use std::collections::VecDeque;
use ferrum_core::types::{IpcCommand, IpcResponse, LogEvent, Position};

const SOCK_PATH: &str = "/tmp/ferrum.sock";

pub struct IpcClient {
    stream:     UnixStream,
    log_buffer: VecDeque<LogEvent>,
}

impl IpcClient {
    pub async fn connect() -> Option<Self> {
        match UnixStream::connect(SOCK_PATH).await {
            Ok(stream) => Some(Self { stream, log_buffer: VecDeque::new() }),
            Err(_)     => None,
        }
    }

    async fn send_cmd(&mut self, cmd: IpcCommand) -> Result<IpcResponse, Box<dyn std::error::Error>> {
        let mut msg = serde_json::to_string(&cmd)?;
        msg.push('\n');
        self.stream.write_all(msg.as_bytes()).await?;

        // Read one line response.
        let mut line = String::new();
        let mut reader = BufReader::new(&mut self.stream);
        reader.read_line(&mut line).await?;
        let resp = serde_json::from_str::<IpcResponse>(line.trim())?;

        // Buffer any log events embedded in the stream.
        if let IpcResponse::LogEvent(ev) = resp {
            self.log_buffer.push_back(ev);
            // Re-request status if we got a log event instead.
            return Err("got log event, not response".into());
        }

        Ok(resp)
    }

    pub async fn request_status(&mut self) -> Result<IpcResponse, Box<dyn std::error::Error>> {
        self.send_cmd(IpcCommand::Status).await
    }

    pub async fn request_fills(&mut self) -> Result<IpcResponse, Box<dyn std::error::Error>> {
        self.send_cmd(IpcCommand::GetFills).await
    }

    pub async fn request_pnl(&mut self, period: &str) -> Result<IpcResponse, Box<dyn std::error::Error>> {
        self.send_cmd(IpcCommand::GetPnl { period: period.to_string() }).await
    }

    pub async fn send_start(&mut self) -> Result<IpcResponse, Box<dyn std::error::Error>> {
        self.send_cmd(IpcCommand::Start).await
    }

    pub async fn send_stop(&mut self) -> Result<IpcResponse, Box<dyn std::error::Error>> {
        self.send_cmd(IpcCommand::Stop).await
    }

    pub async fn request_positions(&mut self) -> Result<Vec<Position>, Box<dyn std::error::Error>> {
        match self.send_cmd(IpcCommand::GetPositions).await? {
            IpcResponse::Positions { positions } => Ok(positions),
            other => Err(format!("unexpected response: {other:?}").into()),
        }
    }

    pub async fn request_pdt(&mut self) -> Result<(u32, u32), Box<dyn std::error::Error>> {
        match self.send_cmd(IpcCommand::GetPdt).await? {
            IpcResponse::PdtStatus { used, max } => Ok((used, max)),
            other => Err(format!("unexpected response: {other:?}").into()),
        }
    }

    /// Drain any buffered log events.
    pub fn poll_log_event(&mut self) -> Option<LogEvent> {
        self.log_buffer.pop_front()
    }
}
