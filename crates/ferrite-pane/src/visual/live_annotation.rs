//! The live guest editor submits metadata before Ferrite captures immutable evidence.
use ferrite_core::browser_annotations::{LiveFeedback, Rect, Viewport};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Submission {
    #[default]
    Attach,
    Send,
}

#[derive(Clone, Debug)]
pub enum Event {
    Ready,
    Attach {
        feedback: LiveFeedback,
        viewport: Viewport,
        submission: Submission,
    },
    Cancelled,
    Closed,
    Failed(String),
    RestoreFailed(String),
}

pub fn crop(png: &[u8], viewport: &Viewport, bounds: &Rect) -> Result<(Vec<u8>, Rect), String> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    if (f64::from(image.width()) - viewport.width_css * viewport.device_scale).abs() > 1.
        || (f64::from(image.height()) - viewport.height_css * viewport.device_scale).abs() > 1.
    {
        return Err("The page size changed before the screenshot could be captured".into());
    }
    let scale = viewport.device_scale;
    let left = (bounds.x.max(0.) * scale)
        .floor()
        .min(f64::from(image.width())) as u32;
    let top = (bounds.y.max(0.) * scale)
        .floor()
        .min(f64::from(image.height())) as u32;
    let right = ((bounds.x + bounds.width).min(viewport.width_css) * scale)
        .ceil()
        .max(0.)
        .min(f64::from(image.width())) as u32;
    let bottom = ((bounds.y + bounds.height).min(viewport.height_css) * scale)
        .ceil()
        .max(0.)
        .min(f64::from(image.height())) as u32;
    if right <= left || bottom <= top {
        return Err("The annotation targets are outside the captured viewport".into());
    }
    let width = right - left;
    let height = bottom - top;
    let mut output = std::io::Cursor::new(Vec::new());
    image
        .crop_imm(left, top, width, height)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok((
        output.into_inner(),
        Rect {
            x: f64::from(left) / scale,
            y: f64::from(top) / scale,
            width: f64::from(width) / scale,
            height: f64::from(height) / scale,
        },
    ))
}

pub(crate) fn parse_event(mut event: serde_json::Value) -> Result<Event, String> {
    match event["kind"].as_str() {
        Some("attach") => {
            let bounds = event["bounds"].take();
            event["feedback"]["bounds"] = bounds;
            event["feedback"]["screenshot"] = serde_json::json!({
                "kind": "unavailable", "reason": "The screenshot has not been captured yet"
            });
            let feedback = serde_json::from_value::<LiveFeedback>(event["feedback"].take())
                .map_err(|error| error.to_string())?;
            ferrite_core::browser_annotations::validate_live_feedback(&feedback)
                .map_err(|error| error.to_string())?;
            let viewport = serde_json::from_value::<Viewport>(event["viewport"].take())
                .map_err(|error| error.to_string())?;
            if ![
                viewport.width_css,
                viewport.height_css,
                viewport.device_scale,
            ]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.)
                || viewport.device_scale > 8.
            {
                return Err("Invalid annotation viewport".into());
            }
            let submission = match event["submission"].as_str() {
                Some("attach") => Submission::Attach,
                Some("send") => Submission::Send,
                _ => return Err("Invalid annotation submission".into()),
            };
            Ok(Event::Attach {
                feedback,
                viewport,
                submission,
            })
        }
        Some("cancelled") => Ok(Event::Cancelled),
        Some("failed") => Ok(Event::Failed(
            event["message"]
                .as_str()
                .unwrap_or("The annotation could not be submitted")
                .to_owned(),
        )),
        _ => Err("Invalid private annotation event".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Vec<u8> {
        let image =
            image::RgbaImage::from_fn(20, 16, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        png.into_inner()
    }

    #[test]
    fn crop_clips_css_union_and_retains_the_selected_pixels() {
        let viewport = Viewport {
            width_css: 10.,
            height_css: 8.,
            device_scale: 2.,
        };
        let (png, bounds) = crop(
            &image(),
            &viewport,
            &Rect {
                x: -2.,
                y: 3.,
                width: 6.,
                height: 9.,
            },
        )
        .unwrap();
        assert_eq!(
            bounds,
            Rect {
                x: 0.,
                y: 3.,
                width: 4.,
                height: 5.
            }
        );
        let image = image::load_from_memory(&png).unwrap().into_rgba8();
        assert_eq!(image.dimensions(), (8, 10));
        assert_eq!(image.get_pixel(0, 0).0, [0, 6, 0, 255]);
        assert_eq!(image.get_pixel(7, 9).0, [7, 15, 0, 255]);
    }

    #[test]
    fn offscreen_targets_do_not_invent_a_screenshot() {
        let viewport = Viewport {
            width_css: 10.,
            height_css: 8.,
            device_scale: 2.,
        };
        assert!(crop(
            &image(),
            &viewport,
            &Rect {
                x: 12.,
                y: 1.,
                width: 4.,
                height: 3.
            }
        )
        .is_err());
        assert!(crop(
            &image(),
            &Viewport {
                device_scale: 1.,
                ..viewport
            },
            &Rect {
                x: 1.,
                y: 1.,
                width: 4.,
                height: 3.
            }
        )
        .is_err());
    }
}
