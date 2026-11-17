use super::{profile, secret, socket, verify_identity};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    path::Path,
    time::Duration,
};
use tokio_tungstenite::tungstenite::Message;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Command {
    Open {
        cwd: String,
        columns: u16,
        rows: u16,
    },
    Resize {
        columns: u16,
        rows: u16,
    },
    Close,
}

pub fn dispatch(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) != Some("--remote-terminal") {
        return None;
    }
    let result = (|| {
        if args.len() != 5 {
            return Err(io::Error::other(
                "The terminal relay needs settings, environment, and directory.",
            ));
        }
        let profile = profile(Path::new(&args[2]), &args[3])?;
        verify_identity(&profile)?;
        let secret = secret(&profile)?;
        let runtime = socket::runtime()?;
        runtime.block_on(async {
            let mut socket = socket::connect(&profile, &secret, "/v1/terminal").await?;
            let _raw = console::raw()?;
            let mut size = console::size();
            socket.send(Message::Text(serde_json::to_string(&Command::Open { cwd: args[4].clone(), columns: size.0, rows: size.1 })?.into())).await.map_err(io::Error::other)?;
            let (input, mut receive) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
            std::thread::spawn(move || {
                let mut buffer = vec![0; 16 * 1024];
                while let Ok(count) = io::stdin().read(&mut buffer) {
                    if count == 0 || input.blocking_send(buffer[..count].to_vec()).is_err() { break; }
                }
            });
            let mut clock = tokio::time::interval(Duration::from_millis(100));
            loop {
                tokio::select! {
                    _ = clock.tick() => {
                        let next = console::size();
                        if next != size {
                            size = next;
                            socket.send(Message::Text(serde_json::to_string(&Command::Resize { columns: size.0, rows: size.1 })?.into())).await.map_err(io::Error::other)?;
                        }
                    },
                    bytes = receive.recv() => match bytes {
                        Some(bytes) => socket.send(Message::Binary(bytes.into())).await.map_err(io::Error::other)?,
                        None => { let _ = socket.close(None).await; return Ok(0); },
                    },
                    message = socket.next() => match message {
                        Some(Ok(Message::Binary(bytes))) => { io::stdout().write_all(&bytes)?; io::stdout().flush()?; },
                        Some(Ok(Message::Text(text))) => {
                            let event: serde_json::Value = serde_json::from_str(&text)?;
                            if event["type"] == "exit" { return Ok(event["code"].as_i64().unwrap_or(1) as i32); }
                            return Err(io::Error::other(event["message"].as_str().unwrap_or("The remote terminal ended.")));
                        },
                        Some(Ok(Message::Ping(_))) => { socket.flush().await.map_err(io::Error::other)?; },
                        Some(Ok(Message::Pong(_))) => {},
                        _ => return Err(io::Error::other("The remote terminal disconnected.")),
                    },
                }
            }
        })
    })();
    Some(match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("\r\n{error}");
            1
        }
    })
}

#[cfg(unix)]
mod console {
    use std::io;
    pub struct Raw(libc::termios);
    pub fn raw() -> io::Result<Raw> {
        let mut original = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut original) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut raw = original;
        unsafe {
            libc::cfmakeraw(&mut raw);
        }
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Raw(original))
    }
    pub fn size() -> (u16, u16) {
        let mut size: libc::winsize = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(libc::STDIN_FILENO, libc::TIOCGWINSZ, &mut size) } == 0 {
            (size.ws_col.max(1), size.ws_row.max(1))
        } else {
            (80, 24)
        }
    }
    impl Drop for Raw {
        fn drop(&mut self) {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.0);
            }
        }
    }
}

#[cfg(windows)]
mod console {
    use std::io;
    use windows_sys::Win32::System::Console::*;
    pub struct Raw {
        input: u32,
        output: u32,
    }
    pub fn raw() -> io::Result<Raw> {
        let mut saved = Raw {
            input: 0,
            output: 0,
        };
        unsafe {
            let input = GetStdHandle(STD_INPUT_HANDLE);
            let output = GetStdHandle(STD_OUTPUT_HANDLE);
            if GetConsoleMode(input, &mut saved.input) == 0
                || GetConsoleMode(output, &mut saved.output) == 0
            {
                return Err(io::Error::last_os_error());
            }
            if SetConsoleMode(input, ENABLE_VIRTUAL_TERMINAL_INPUT) == 0
                || SetConsoleMode(
                    output,
                    ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
                ) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(saved)
    }
    pub fn size() -> (u16, u16) {
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
        if unsafe { GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) } != 0 {
            (
                (info.srWindow.Right - info.srWindow.Left + 1) as u16,
                (info.srWindow.Bottom - info.srWindow.Top + 1) as u16,
            )
        } else {
            (80, 24)
        }
    }
    impl Drop for Raw {
        fn drop(&mut self) {
            unsafe {
                SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), self.input);
                SetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), self.output);
            }
        }
    }
}
