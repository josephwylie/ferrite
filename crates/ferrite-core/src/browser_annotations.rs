//! Immutable Browser feedback carried by the ordinary prompt/history codec.
//! Captures outlive the live page. DOM observations are evidence, never source
//! locations or handles to resolve again after the page changes.
use crate::{browser::Document, prompt_files};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub width_css: f64,
    pub height_css: f64,
    pub device_scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Logical viewport coordinates, not document or screenshot pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ElementContext {
    pub tag: String,
    pub role: Option<String>,
    pub name: Option<String>,
    pub text: Option<String>,
    pub bounds_css: Rect,
    pub selector_hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Selection {
    Region {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Element {
        context: ElementContext,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub document: Document,
    pub native_generation: u64,
    pub url: String,
    pub viewport: Viewport,
    pub selection: Selection,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ink {
    /// Six-digit RGB, including the leading '#'.
    pub color: String,
    pub width: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Drawing {
    Pen { points: Vec<Point>, ink: Ink },
    Line { from: Point, to: Point, ink: Ink },
    Arrow { from: Point, to: Point, ink: Ink },
    Rectangle { bounds: Rect, ink: Ink },
    Ellipse { bounds: Rect, ink: Ink },
    Text { at: Point, text: String, ink: Ink },
    Step { at: Point, number: u32, ink: Ink },
}

/// Once added to the Composer the original and captured context cannot be
/// reassigned. A new capture creates another annotation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    id: String,
    capture: Capture,
    original: PathBuf,
    original_sha256: String,
    pixel_width: u32,
    pixel_height: u32,
    comment: String,
    drawings: Vec<Drawing>,
}

impl Annotation {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn capture(&self) -> &Capture {
        &self.capture
    }
    pub fn original(&self) -> &Path {
        &self.original
    }
    pub fn comment(&self) -> &str {
        &self.comment
    }
    pub fn drawings(&self) -> &[Drawing] {
        &self.drawings
    }
    pub fn pixel_size(&self) -> (u32, u32) {
        (self.pixel_width, self.pixel_height)
    }
}

/// Retain a capture under the application's store directory before it enters
/// a draft. Content-addressed files are never overwritten or deleted on removal
/// from a draft: an earlier queued/sent message may still refer to them.
pub fn retain(
    store: &Path,
    capture: Capture,
    png: &[u8],
    comment: String,
    drawings: Vec<Drawing>,
) -> io::Result<Annotation> {
    let (pixel_width, pixel_height) = png_size(png)?;
    let hash = format!("{:x}", Sha256::digest(png));
    let directory = store.join("browser-annotations");
    fs::create_dir_all(&directory)?;
    let original = fs::canonicalize(&directory)?.join(format!("{hash}.png"));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&original)
    {
        Ok(mut file) => {
            if let Err(error) = file.write_all(png).and_then(|_| file.sync_all()) {
                let _ = fs::remove_file(&original);
                return Err(error);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if fs::read(&original)? != png {
                return Err(io::Error::other(
                    "The retained browser capture has changed; capture the page again",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    let identity = serde_json::to_vec(&(&capture, &hash, &comment, &drawings))?;
    Ok(Annotation {
        id: format!("{:x}", Sha256::digest(identity)),
        capture,
        original,
        original_sha256: hash,
        pixel_width,
        pixel_height,
        comment,
        drawings,
    })
}

fn png_size(png: &[u8]) -> io::Result<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return Err(io::Error::other(
            "The browser capture is not a PNG; capture the page again",
        ));
    }
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    if width == 0 || height == 0 {
        return Err(io::Error::other(
            "The browser capture is empty; capture the page again",
        ));
    }
    Ok((width, height))
}

const START: &str = "<ferrite_browser_annotations version=\"1\">\n";
const END: &str = "\n</ferrite_browser_annotations>";

/// Both providers receive context as text and the same retained originals as
/// native image references. The queue and Store continue to carry one string.
pub fn compose(text: &str, files: &[PathBuf], annotations: &[Annotation]) -> String {
    let mut text = text.to_owned();
    let mut files = files.to_vec();
    if !annotations.is_empty() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(START);
        text.push_str(
            &serde_json::to_string(annotations).expect("annotation contains serializable values"),
        );
        text.push_str(END);
        for annotation in annotations {
            if !files.iter().any(|path| path == annotation.original()) {
                files.push(annotation.original().to_owned());
            }
        }
    }
    prompt_files::compose(&text, &files)
}

/// Unknown or malformed context remains ordinary prompt text and file refs;
/// history from a newer version must never silently lose feedback.
pub fn split(prompt: String) -> (String, Vec<PathBuf>, Vec<Annotation>) {
    let (text, mut files) = prompt_files::split(prompt);
    let Some(body) = text.strip_suffix(END) else {
        return (text, files, vec![]);
    };
    let Some(at) = body.rfind(START) else {
        return (text, files, vec![]);
    };
    if at != 0 && !body[..at].ends_with("\n\n") {
        return (text, files, vec![]);
    }
    let Ok(annotations) = serde_json::from_str::<Vec<Annotation>>(&body[at + START.len()..]) else {
        return (text, files, vec![]);
    };
    files.retain(|path| {
        !annotations
            .iter()
            .any(|annotation| path == annotation.original())
    });
    let prose = if at == 0 { "" } else { &text[..at - 2] };
    (prose.to_owned(), files, annotations)
}
