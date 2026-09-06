//! Spawning children without flashing a console window.

use std::process::Command;

/// `CREATE_NO_WINDOW`: the child gets no console of its own.
///
/// Every provider and tool child here talks over piped stdio, never a
/// terminal — but a GUI process spawning a console program (or, for the
/// npm `.cmd` shims, the `cmd.exe` std runs them through) makes Windows
/// allocate a console window for it, which flashes over the cockpit.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Suppress the console window a spawned child would otherwise get.
///
/// A no-op off Windows, so call sites stay free of `cfg`.
pub trait NoConsoleWindow {
    fn no_console_window(&mut self) -> &mut Self;
}

impl NoConsoleWindow for Command {
    fn no_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}
