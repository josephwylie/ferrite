//! Immutable Browser feedback carried by the ordinary prompt/history codec.
//! Captures outlive the live page. DOM observations are evidence, never source
//! locations or handles to resolve again after the page changes.
use crate::{browser::Document, prompt_files};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read, Write},
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
    #[serde(other)]
    Unknown,
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
    Pen {
        points: Vec<Point>,
        ink: Ink,
    },
    Line {
        from: Point,
        to: Point,
        ink: Ink,
    },
    Arrow {
        from: Point,
        to: Point,
        ink: Ink,
    },
    Rectangle {
        bounds: Rect,
        ink: Ink,
    },
    Ellipse {
        bounds: Rect,
        ink: Ink,
    },
    Text {
        at: Point,
        text: String,
        ink: Ink,
    },
    Step {
        at: Point,
        number: u32,
        ink: Ink,
    },
    #[serde(other)]
    Unknown,
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
    pub fn original_hash(&self) -> &str {
        &self.original_sha256
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

    /// A window can cache the image for display without replacing the Server
    /// path serialized to providers and history.
    pub fn cache_image(&self, png: &[u8]) -> io::Result<PathBuf> {
        if png_size(png)? != self.pixel_size()
            || format!("{:x}", Sha256::digest(png)) != self.original_sha256
        {
            return Err(io::Error::other(
                "The retained capture could not be verified; reopen it to try again",
            ));
        }
        let image = retain(
            &std::env::temp_dir().join("ferrite-browser-capture-cache"),
            self.capture.clone(),
            png,
            String::new(),
            vec![],
        )?;
        Ok(image.original)
    }

    pub fn cached_image(&self) -> Option<PathBuf> {
        if !valid_hash(&self.original_sha256) {
            return None;
        }
        let path = std::env::temp_dir()
            .join("ferrite-browser-capture-cache")
            .join("browser-annotations")
            .join(format!("{}.png", self.original_sha256));
        path.is_file().then_some(path)
    }

    pub(crate) fn local_image(&self) -> io::Result<Vec<u8>> {
        let parent = self.original.parent().filter(|parent| {
            parent
                .file_name()
                .is_some_and(|name| name == "browser-annotations")
        });
        if self.original.file_name().is_none_or(|name| {
            name != std::ffi::OsStr::new(&format!("{}.png", self.original_sha256))
        }) {
            return Err(io::Error::other("This browser capture is unavailable"));
        }
        let directory = parent
            .and_then(Path::parent)
            .ok_or_else(|| io::Error::other("This browser capture is unavailable"))?;
        read_retained(directory, &self.original_sha256)
    }

    pub fn cache_wire_image(&self, png: &crate::protocol::WirePng) -> io::Result<PathBuf> {
        use base64::Engine as _;
        if png.base64.len() > ((MAX_CAPTURE_BYTES + 2) / 3) * 4
            || (png.width, png.height) != self.pixel_size()
        {
            return Err(io::Error::other(
                "The retained capture geometry could not be verified",
            ));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&png.base64)
            .map_err(io::Error::other)?;
        self.cache_image(&bytes)
    }

    /// Finalize feedback against the retained evidence. This never consults a
    /// live tab, so navigation after capture cannot reassign the annotation.
    pub fn with_feedback(&self, comment: String, drawings: Vec<Drawing>) -> io::Result<Self> {
        validate_drawings(&drawings)?;
        let mut annotation = self.clone();
        annotation.id = identity(&self.capture, &self.original_sha256, &comment, &drawings)?;
        annotation.comment = comment;
        annotation.drawings = drawings;
        Ok(annotation)
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
    validate_capture(&capture, pixel_width, pixel_height)?;
    validate_drawings(&drawings)?;
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
    #[cfg(unix)]
    fs::File::open(&directory)?.sync_all()?;
    let id = identity(&capture, &hash, &comment, &drawings)?;
    Ok(Annotation {
        id,
        capture,
        original,
        original_sha256: hash,
        pixel_width,
        pixel_height,
        comment,
        drawings,
    })
}

/// Fits inside the existing 64 MiB JSON-lines transport after base64 encoding.
pub const MAX_CAPTURE_BYTES: usize = 32 * 1024 * 1024;

/// Encode only a bounded PNG for the existing retained-capture transport.
pub fn wire_png(png: &[u8]) -> io::Result<crate::protocol::WirePng> {
    use base64::Engine as _;
    let (width, height) = png_size(png)?;
    Ok(crate::protocol::WirePng {
        width,
        height,
        base64: base64::engine::general_purpose::STANDARD.encode(png),
    })
}

pub(crate) fn png_size(png: &[u8]) -> io::Result<(u32, u32)> {
    if png.len() > MAX_CAPTURE_BYTES {
        return Err(io::Error::other(
            "The browser capture is too large; use a smaller viewport and capture again",
        ));
    }
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return Err(io::Error::other(
            "The browser capture is not a PNG; capture the page again",
        ));
    }
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 64 * 1024 * 1024 {
        return Err(io::Error::other(
            "The browser capture is empty; capture the page again",
        ));
    }
    Ok((width, height))
}

fn identity(
    capture: &Capture,
    hash: &str,
    comment: &str,
    drawings: &[Drawing],
) -> io::Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(capture, hash, comment, drawings))?)
    ))
}

fn valid_rect(rect: &Rect) -> bool {
    [rect.x, rect.y, rect.width, rect.height]
        .into_iter()
        .all(f64::is_finite)
        && rect.width > 0.0
        && rect.height > 0.0
}

fn validate_capture(capture: &Capture, width: u32, height: u32) -> io::Result<()> {
    let viewport = &capture.viewport;
    let valid_viewport = [
        viewport.width_css,
        viewport.height_css,
        viewport.device_scale,
    ]
    .into_iter()
    .all(|value| value.is_finite() && value > 0.0)
        && viewport.device_scale <= 8.0
        && (viewport.width_css * viewport.device_scale - f64::from(width)).abs() <= 1.0
        && (viewport.height_css * viewport.device_scale - f64::from(height)).abs() <= 1.0;
    let bounds = match &capture.selection {
        Selection::Region {
            x,
            y,
            width,
            height,
        } => Rect {
            x: *x,
            y: *y,
            width: *width,
            height: *height,
        },
        Selection::Element { context } => context.bounds_css.clone(),
        Selection::Unknown => return Err(io::Error::other("Unsupported browser selection")),
    };
    let valid_selection = valid_rect(&bounds)
        && bounds.x < viewport.width_css
        && bounds.y < viewport.height_css
        && bounds.x + bounds.width > 0.0
        && bounds.y + bounds.height > 0.0;
    let valid_url = url::Url::parse(&capture.url)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some());
    if !valid_viewport || !valid_selection || !valid_url {
        return Err(io::Error::other(
            "The page geometry changed; select the element or region again",
        ));
    }
    Ok(())
}

fn validate_drawings(drawings: &[Drawing]) -> io::Result<()> {
    let point = |point: &Point| point.x.is_finite() && point.y.is_finite();
    for drawing in drawings {
        let (valid, ink) = match drawing {
            Drawing::Unknown => return Err(io::Error::other("Unsupported browser drawing")),
            Drawing::Pen { points, ink } => (!points.is_empty() && points.iter().all(&point), ink),
            Drawing::Line { from, to, ink } | Drawing::Arrow { from, to, ink } => {
                (point(from) && point(to), ink)
            }
            Drawing::Rectangle { bounds, ink } | Drawing::Ellipse { bounds, ink } => {
                (valid_rect(bounds), ink)
            }
            Drawing::Text { at, ink, .. } => (point(at), ink),
            Drawing::Step { at, number, ink } => (point(at) && *number > 0, ink),
        };
        if !valid
            || !ink.width.is_finite()
            || ink.width <= 0.0
            || ink.color.len() != 7
            || !ink.color.starts_with('#')
            || !ink.color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(io::Error::other(
                "The drawing could not be saved; undo the last mark and try again",
            ));
        }
    }
    Ok(())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Fetch only an existing retained PNG, addressed by content hash. A remote
/// window never supplies a filesystem path to this boundary.
pub fn read_retained(thread_directory: &Path, hash: &str) -> io::Result<Vec<u8>> {
    if !valid_hash(hash) {
        return Err(io::Error::other("This browser capture is unavailable"));
    }
    let file = fs::File::open(
        thread_directory
            .join("browser-annotations")
            .join(format!("{hash}.png")),
    )?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("This browser capture is unavailable"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_CAPTURE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    png_size(&bytes)?;
    if format!("{:x}", Sha256::digest(&bytes)) != hash {
        return Err(io::Error::other(
            "The retained browser capture has changed; its original cannot be verified",
        ));
    }
    Ok(bytes)
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
        // Page-derived strings must remain data even when they contain prompt
        // file tokens or codec delimiter lookalikes. JSON decoding restores
        // the exact text; only our explicit image trailer becomes attachments.
        let context = serde_json::to_string(annotations)
            .expect("annotation contains serializable values")
            .replace('@', "\\u0040")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e");
        text.push_str(&context);
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
    if annotations.iter().any(|annotation| {
        matches!(annotation.capture.selection, Selection::Unknown)
            || annotation
                .drawings
                .iter()
                .any(|drawing| matches!(drawing, Drawing::Unknown))
    }) {
        return (text, files, vec![]);
    }
    files.retain(|path| {
        !annotations
            .iter()
            .any(|annotation| path == annotation.original())
    });
    let prose = if at == 0 { "" } else { &text[..at - 2] };
    (prose.to_owned(), files, annotations)
}
