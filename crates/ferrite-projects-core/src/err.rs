//! What goes wrong: one error type for the whole program, a sentence for a person and what kind of failure it was.
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Plain,
    /// Somebody else holds the lock: try again in a moment.
    Busy,
    /// A value or a request was refused, with the reason (and the nearest right one) in the words.
    Refused,
}
#[derive(Debug, Clone)]
pub struct Error { pub msg: String, pub kind: Kind }
pub type Res<T> = Result<T, Error>;

impl Error {
    pub fn new(msg: impl Into<String>) -> Self { Error { msg: msg.into(), kind: Kind::Plain } }
    pub fn refused(msg: impl Into<String>) -> Self { Error { msg: msg.into(), kind: Kind::Refused } }
    pub fn busy(msg: impl Into<String>) -> Self { Error { msg: msg.into(), kind: Kind::Busy } }
}
impl fmt::Display for Error { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { f.write_str(&self.msg) } }
impl std::error::Error for Error {}
impl From<std::io::Error> for Error { fn from(e: std::io::Error) -> Self { Error::new(e.to_string()) } }
impl From<serde_json::Error> for Error { fn from(e: serde_json::Error) -> Self { Error::new(e.to_string()) } }
impl From<String> for Error { fn from(e: String) -> Self { Error::new(e) } }
impl From<&str> for Error { fn from(e: &str) -> Self { Error::new(e) } }

/// Return an error made of these words.
#[macro_export]
macro_rules! fail { ($($t:tt)*) => { return Err($crate::err::Error::new(format!($($t)*))) } }
/// Return a refusal made of these words.
#[macro_export]
macro_rules! refuse { ($($t:tt)*) => { return Err($crate::err::Error::refused(format!($($t)*))) } }
