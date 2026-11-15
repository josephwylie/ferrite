//! Where the operator left the window, and where it can open now. The
//! frame is kept in its own file beside the settings: the window writes it
//! while the Cockpit owns `settings.json`, and two writers of one file would
//! overwrite each other. A monitor that has gone since the frame was saved
//! must not take the window with it, so `fitted` always lands it on a
//! display that exists.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

/// A rectangle in logical pixels, the origin at its top left.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Frame {
    fn right(&self) -> f32 {
        self.x + self.width
    }

    fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn overlap(&self, other: &Frame) -> f32 {
        let width = self.right().min(other.right()) - self.x.max(other.x);
        let height = self.bottom().min(other.bottom()) - self.y.max(other.y);
        width.max(0.0) * height.max(0.0)
    }
}

/// The window as it was left: its frame (the restore frame while
/// maximized) and whether it was maximized.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowFrame {
    pub frame: Frame,
    pub maximized: bool,
}

impl WindowFrame {
    const FILE: &'static str = "window.json";

    /// The saved frame; none when there is none or it cannot be read, the
    /// window then opening where it would have.
    pub fn load(dir: &Path) -> Option<WindowFrame> {
        serde_json::from_slice(&std::fs::read(dir.join(Self::FILE)).ok()?).ok()
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        crate::store::replace_file(
            &dir.join(Self::FILE),
            &serde_json::to_vec(self).map_err(io::Error::other)?,
        )
    }

    /// The frame to open at, given the visible area of every display now
    /// (the primary first) and the smallest size the window works at. It
    /// goes to the display it overlapped most, sized to fit and shifted
    /// whole inside; one that overlaps none (its monitor is gone) is
    /// centred on the primary. None with no display at all.
    pub fn fitted(&self, displays: &[Frame], min: (f32, f32)) -> Option<WindowFrame> {
        let primary = displays.first()?;
        let wanted = &self.frame;
        let (display, seen) = displays
            .iter()
            .map(|display| (display, display.overlap(wanted)))
            .fold((primary, 0.0), |best, next| if next.1 > best.1 { next } else { best });
        let width = wanted.width.max(min.0).min(display.width);
        let height = wanted.height.max(min.1).min(display.height);
        let (x, y) = if seen > 0.0 {
            (
                wanted.x.clamp(display.x, display.right() - width),
                wanted.y.clamp(display.y, display.bottom() - height),
            )
        } else {
            (
                display.x + (display.width - width) / 2.0,
                display.y + (display.height - height) / 2.0,
            )
        };
        Some(WindowFrame {
            frame: Frame { x, y, width, height },
            maximized: self.maximized,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: Frame = Frame { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 };
    const EXTERNAL: Frame = Frame { x: 1440.0, y: 0.0, width: 2560.0, height: 1440.0 };
    const MIN: (f32, f32) = (640.0, 420.0);

    fn at(x: f32, y: f32, width: f32, height: f32) -> WindowFrame {
        WindowFrame { frame: Frame { x, y, width, height }, maximized: false }
    }

    #[test]
    fn a_frame_on_a_display_stays_where_it_was() {
        let saved = at(1800.0, 100.0, 1200.0, 800.0);
        assert_eq!(saved.fitted(&[LAPTOP, EXTERNAL], MIN), Some(saved));
    }

    #[test]
    fn a_window_on_a_monitor_that_is_gone_is_centred_on_the_primary() {
        let saved = at(1800.0, 100.0, 1200.0, 800.0);
        let fitted = saved.fitted(&[LAPTOP], MIN).unwrap();
        assert_eq!(fitted.frame, Frame { x: 120.0, y: 50.0, width: 1200.0, height: 800.0 });
    }

    #[test]
    fn a_frame_half_off_a_display_is_shifted_whole_inside() {
        let saved = at(1000.0, 700.0, 800.0, 600.0);
        let fitted = saved.fitted(&[LAPTOP], MIN).unwrap();
        assert_eq!(fitted.frame, Frame { x: 640.0, y: 300.0, width: 800.0, height: 600.0 });
    }

    #[test]
    fn a_frame_larger_than_its_display_shrinks_to_it() {
        let saved = at(0.0, 0.0, 3000.0, 2000.0);
        let fitted = saved.fitted(&[LAPTOP], MIN).unwrap();
        assert_eq!(fitted.frame, LAPTOP);
    }

    #[test]
    fn a_frame_below_the_minimum_grows_to_it() {
        let saved = at(10.0, 10.0, 100.0, 100.0);
        let fitted = saved.fitted(&[LAPTOP], MIN).unwrap();
        assert_eq!((fitted.frame.width, fitted.frame.height), MIN);
    }

    #[test]
    fn the_display_it_overlapped_most_takes_it() {
        let saved = at(1300.0, 100.0, 1000.0, 600.0);
        let fitted = saved.fitted(&[LAPTOP, EXTERNAL], MIN).unwrap();
        assert!(fitted.frame.x >= EXTERNAL.x, "{fitted:?}");
    }

    #[test]
    fn maximized_is_kept_and_no_display_gives_no_frame() {
        let saved = WindowFrame { maximized: true, ..at(0.0, 0.0, 900.0, 700.0) };
        assert!(saved.fitted(&[LAPTOP], MIN).unwrap().maximized);
        assert_eq!(saved.fitted(&[], MIN), None);
    }

    #[test]
    fn a_saved_frame_loads_back() {
        let dir = std::env::temp_dir().join(format!("ferrite-window-{}", std::process::id()));
        let saved = at(10.0, 20.0, 800.0, 600.0);
        assert_eq!(WindowFrame::load(&dir), None);
        saved.save(&dir).unwrap();
        assert_eq!(WindowFrame::load(&dir), Some(saved));
        std::fs::write(dir.join("window.json"), "garbage").unwrap();
        assert_eq!(WindowFrame::load(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
