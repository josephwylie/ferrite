use super::{Profile, FRAME_LIMIT, TIMEOUT};
use crate::machine::{Failure, Tunnel};
use futures_util::{SinkExt, StreamExt};
use std::io::{self, Cursor, Read, Write};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

pub type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub async fn connect(profile: &Profile, secret: &str, path: &str) -> io::Result<Socket> {
    super::configure_tls();
    let origin = profile
        .origin
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let mut request = format!("{origin}{path}")
        .into_client_request()
        .map_err(io::Error::other)?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {secret}")
            .parse()
            .map_err(io::Error::other)?,
    );
    let mut config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
    config.max_message_size = Some(FRAME_LIMIT);
    config.max_frame_size = Some(FRAME_LIMIT);
    let (socket, response) = tokio::time::timeout(TIMEOUT, tokio_tungstenite::connect_async_with_config(request, Some(config), false)).await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "The remote link timed out."))?
        .map_err(|error| {
            match &error {
                tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 401 => {
                    io::Error::new(io::ErrorKind::PermissionDenied, Failure::Revoked)
                }
                _ => io::Error::other("The remote link was refused. Check the certificate or pair again if access was revoked."),
            }
        })?;
    if response
        .headers()
        .get("x-ferrite-environment")
        .and_then(|value| value.to_str().ok())
        != Some(&profile.server_id)
    {
        return Err(io::Error::other(
            "The authenticated remote environment identity does not match.",
        ));
    }
    Ok(socket)
}

pub fn runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

pub fn tunnel(profile: &Profile, secret: &str, path: &str) -> io::Result<Tunnel> {
    let runtime = runtime()?;
    let mut socket = runtime.block_on(connect(profile, secret, path))?;
    let (input, mut incoming) = mpsc::channel::<Vec<u8>>(8);
    let (output, outgoing) = mpsc::channel::<Vec<u8>>(8);
    let worker = std::thread::Builder::new().name("ferrite-link".into()).spawn(move || {
        runtime.block_on(async move {
            loop {
                tokio::select! {
                    _ = output.closed() => break,
                    data = incoming.recv() => match data {
                        Some(data) => if socket.send(Message::Binary(data.into())).await.is_err() { break; },
                        None => break,
                    },
                    message = socket.next() => match message {
                        Some(Ok(Message::Binary(data))) => if output.send(data.to_vec()).await.is_err() { break; },
                        Some(Ok(Message::Ping(_))) => { let _ = socket.flush().await; },
                        Some(Ok(Message::Pong(_))) => {},
                        _ => break,
                    },
                }
            }
            let _ = socket.close(None).await;
        });
    })?;
    Ok(Tunnel {
        from: Box::new(ChannelReader {
            receiver: outgoing,
            buffer: Cursor::new(Vec::new()),
        }),
        to: Box::new(ChannelWriter(input)),
        ended: Box::new(move || {
            let _ = worker.join();
            Some(Failure::Unreachable(
                "The remote link closed. Reconnect or pair again if access was revoked.".into(),
            ))
        }),
    })
}

struct ChannelReader {
    receiver: mpsc::Receiver<Vec<u8>>,
    buffer: Cursor<Vec<u8>>,
}
impl Read for ChannelReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        loop {
            let count = self.buffer.read(bytes)?;
            if count != 0 {
                return Ok(count);
            }
            match self.receiver.blocking_recv() {
                Some(data) => self.buffer = Cursor::new(data),
                None => return Ok(0),
            }
        }
    }
}
struct ChannelWriter(mpsc::Sender<Vec<u8>>);
impl Write for ChannelWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = bytes.len().min(FRAME_LIMIT);
        self.0
            .blocking_send(bytes[..count].to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "The remote link closed."))?;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
