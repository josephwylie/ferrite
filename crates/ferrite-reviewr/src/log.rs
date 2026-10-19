//! Debug log: one timestamped line per event to `$HERDR_REVIEW_LOG`; a no-op when unset.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static SINK: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

/// Open the log sink from `$HERDR_REVIEW_LOG` if set. Call once at startup.
pub fn init() {
    SINK.get_or_init(|| {
        let path = std::env::var("HERDR_REVIEW_LOG").ok()?;
        OpenOptions::new().create(true).append(true).open(path).ok().map(Mutex::new)
    });
}

/// Whether a log sink is open.
pub fn enabled() -> bool {
    matches!(SINK.get(), Some(Some(_)))
}

/// Append one formatted line, prefixed with epoch milliseconds.
pub fn write(line: &str) {
    if let Some(Some(sink)) = SINK.get()
        && let Ok(mut file) = sink.lock()
    {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let _ = writeln!(file, "{ts} {line}");
    }
}

/// Log a formatted line when logging is enabled; otherwise do nothing.
#[macro_export]
macro_rules! logln {
    ($($arg:tt)*) => {
        if $crate::log::enabled() {
            $crate::log::write(&format!($($arg)*));
        }
    };
}
