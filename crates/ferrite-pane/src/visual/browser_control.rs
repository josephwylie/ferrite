//! Results from one exact-target browser capture transaction. The capture
//! retains original bytes independently of the live view and its next frame.
use ferrite_core::browser::Document;
use ferrite_core::browser_annotations::{ElementContext, Viewport};

#[derive(Clone, Debug)]
pub struct CapturedPage {
    pub document: Document,
    pub native_generation: u64,
    pub url: String,
    pub viewport: Viewport,
    pub png: Vec<u8>,
    pub element: Option<ElementContext>,
}

#[cfg(feature = "cef")]
pub mod native;

#[cfg(feature = "browser-smoke")]
mod completion_gate;
