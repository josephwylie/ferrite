use super::{send_frame, Gateway};
use crate::machine::remote::terminal::Command;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{CommandBuilder, PtySize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

const INPUT_STALL: Duration = Duration::from_secs(5);

fn size(columns: u16, rows: u16) -> Option<PtySize> {
    ((1..=1000).contains(&columns) && (1..=1000).contains(&rows)).then_some(PtySize {
        rows,
        cols: columns,
        pixel_width: 0,
        pixel_height: 0,
    })
}

pub(super) async fn serve(mut socket: WebSocket, gateway: Gateway, session: String) {
    if let Err(error) = run(&mut socket, &gateway, &session).await {
        let _ = send_frame(&mut socket, Message::Text(
            serde_json::json!({"type":"error","message":error.to_string()})
                .to_string()
                .into(),
        )).await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), socket.close()).await;
}

async fn run(socket: &mut WebSocket, gateway: &Gateway, session: &str) -> std::io::Result<()> {
    let first = tokio::time::timeout(Duration::from_secs(10), socket.next())
        .await
        .map_err(std::io::Error::other)?;
    let Some(Ok(Message::Text(text))) = first else {
        return Err(std::io::Error::other(
            "A terminal open request is required.",
        ));
    };
    let Command::Open { cwd, columns, rows } = serde_json::from_str(&text)? else {
        return Err(std::io::Error::other(
            "A terminal open request is required.",
        ));
    };
    let cwd = PathBuf::from(cwd);
    if !cwd.is_absolute() || !cwd.is_dir() || !gateway.active(session) {
        return Err(std::io::Error::other(
            "The remote terminal directory is unavailable.",
        ));
    }
    let size =
        size(columns, rows).ok_or_else(|| std::io::Error::other("Invalid terminal dimensions."))?;
    let pair = portable_pty::native_pty_system()
        .openpty(size)
        .map_err(std::io::Error::other)?;
    let mut command = match &gateway.shell {
        Some(shell) => CommandBuilder::new(shell),
        None => CommandBuilder::new_default_prog(),
    };
    command.cwd(cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(std::io::Error::other)?;
    let mut child = Child(child);
    drop(pair.slave);
    let mut writer = pair.master.take_writer().map_err(std::io::Error::other)?;
    let (input, mut incoming) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
    tokio::task::spawn_blocking(move || {
        while let Some(bytes) = incoming.blocking_recv() {
            if writer.write_all(&bytes).is_err() {
                break;
            }
        }
    });
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(std::io::Error::other)?;
    let (output, mut receive) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
    tokio::task::spawn_blocking(move || {
        let mut buffer = vec![0; 32 * 1024];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 || output.blocking_send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut clock = tokio::time::interval(Duration::from_millis(100));
    let mut pending: Option<(Vec<u8>, Instant)> = None;
    let mut input_closed = false;
    loop {
        tokio::select! {
            _ = clock.tick() => {
                if !gateway.active(session) { break; }
                if pending.as_ref().is_some_and(|(_, since)| since.elapsed() >= INPUT_STALL) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "The remote terminal stopped accepting input for five seconds.",
                    ));
                }
                if receive.is_closed() && receive.is_empty() {
                    if let Some(status) = child.0.try_wait().map_err(std::io::Error::other)? {
                        send_frame(socket, Message::Text(serde_json::json!({"type":"exit","code":status.exit_code()}).to_string().into())).await?;
                        break;
                    }
                }
            },
            permit = input.reserve(), if pending.is_some() => match permit {
                Ok(permit) => permit.send(pending.take().expect("a pending terminal input frame").0),
                Err(_) => {
                    pending = None;
                    input_closed = true;
                },
            },
            bytes = receive.recv(), if !receive.is_closed() || !receive.is_empty() => match bytes {
                Some(bytes) => send_frame(socket, Message::Binary(bytes.into())).await?,
                None => {},
            },
            message = socket.next(), if pending.is_none() && !input_closed => match message {
                Some(Ok(Message::Binary(bytes))) if gateway.active(session) => pending = Some((bytes.to_vec(), Instant::now())),
                Some(Ok(Message::Text(text))) if gateway.active(session) => match serde_json::from_str::<Command>(&text)? {
                    Command::Resize { columns, rows } => pair.master.resize(self::size(columns, rows).ok_or_else(|| std::io::Error::other("Invalid terminal dimensions."))?).map_err(std::io::Error::other)?,
                    Command::Close => break,
                    Command::Open { .. } => return Err(std::io::Error::other("This terminal is already open.")),
                },
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {},
                _ => break,
            },
        }
    }
    Ok(())
}

struct Child(Box<dyn portable_pty::Child + Send + Sync>);
impl Drop for Child {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.0.process_id() {
            // The PTY child owns its session; close descendants holding the slave too.
            unsafe {
                libc::kill(-(pid as libc::pid_t), libc::SIGHUP);
            }
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
