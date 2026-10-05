//! How the platform rasterises Ferrite's text, set once before the first
//! glyph is drawn.
//!
//! **Why** (F-17). The approved prototype is drawn by a browser with
//! `-webkit-font-smoothing: antialiased`: greyscale coverage, no stem
//! darkening. gpui-pre-macos 0.3.3 thickens every glyph instead
//! (`MacTextSystem::glyph_dilation_for_color`, up to four steps by the
//! ink's luminance) whenever font smoothing is allowed — and it decides that
//! once, in a `OnceLock`, from `AppleFontSmoothing` as
//! `CFPreferencesCopyAppValue(kCFPreferencesCurrentApplication)` reports
//! it: only an explicit `0` turns the dilation off. Light ink on the grey
//! plane is where the dilation is heaviest, so the app's `done`, `needs
//! you` and every title read a weight heavier than the prototype's.
//!
//! **What.** `disable_font_smoothing` sets `AppleFontSmoothing = 0` in the
//! application's own preferences domain (`CFPreferencesSetAppValue`,
//! nothing system-wide) before anything lays out text, so the OnceLock
//! reads `0` and gpui rasterises the prototype's greyscale glyphs. It must
//! run before the first glyph is rasterised — the first call into gpui's
//! text system after `register_fonts` can be the one that caches the answer
//! — so `main` calls it first, and the visual-reference scenes call it
//! before their first render. Every other platform: a no-op.

/// Turn off gpui's stem darkening for this process (see the module doc).
/// Idempotent; safe to call before `App` exists. Only the visual-reference
/// captures use it: on glass the greyscale glyphs read thin and wispy, so
/// the product draws text the way macOS does (`follow_system_font_smoothing`).
pub fn disable_font_smoothing() {
    #[cfg(target_os = "macos")]
    mac::set_app_font_smoothing(Some(0));
}

/// Draw text the way the system does: clear any `AppleFontSmoothing` the app
/// domain holds (an earlier build wrote `0` there), so macOS's own smoothing
/// applies. Call before the first glyph is rasterised.
pub fn follow_system_font_smoothing() {
    #[cfg(target_os = "macos")]
    mac::set_app_font_smoothing(None);
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_void};

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFNumberRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CFIndex = isize;
    type CFNumberType = CFIndex;
    type CFStringEncoding = u32;

    /// `kCFNumberSInt32Type`.
    const NUMBER_SINT32: CFNumberType = 3;
    /// `kCFStringEncodingUTF8`.
    const ENCODING_UTF8: CFStringEncoding = 0x0800_0100;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFPreferencesCurrentApplication: CFStringRef;
        fn CFStringCreateWithCString(
            allocator: CFAllocatorRef,
            text: *const c_char,
            encoding: CFStringEncoding,
        ) -> CFStringRef;
        fn CFNumberCreate(
            allocator: CFAllocatorRef,
            kind: CFNumberType,
            value: *const c_void,
        ) -> CFNumberRef;
        fn CFPreferencesSetAppValue(key: CFStringRef, value: CFTypeRef, application: CFStringRef);
        fn CFRelease(object: CFTypeRef);
    }

    /// `AppleFontSmoothing = value` in the current application's domain;
    /// `None` removes the key, so the system setting applies.
    pub(super) fn set_app_font_smoothing(value: Option<i32>) {
        // SAFETY: plain CoreFoundation calls on objects created here and
        // released here; the key is a NUL-terminated literal, the value a
        // live `i32`, and the domain the framework's own constant.
        unsafe {
            let key = CFStringCreateWithCString(
                std::ptr::null(),
                c"AppleFontSmoothing".as_ptr(),
                ENCODING_UTF8,
            );
            if key.is_null() {
                return;
            }
            match value {
                Some(value) => {
                    let number = CFNumberCreate(
                        std::ptr::null(),
                        NUMBER_SINT32,
                        (&value as *const i32).cast::<c_void>(),
                    );
                    if !number.is_null() {
                        CFPreferencesSetAppValue(key, number, kCFPreferencesCurrentApplication);
                        CFRelease(number);
                    }
                }
                None => CFPreferencesSetAppValue(
                    key,
                    std::ptr::null(),
                    kCFPreferencesCurrentApplication,
                ),
            }
            CFRelease(key);
        }
    }
}

/// The bundled face with its vertical metrics split where the browser
/// sets its baseline (F-18).
///
/// **Why.** gpui centres a face's ascent+descent box in the line: Geist
/// Mono's 1005/295 units put the baseline 14.615px down a 13/20 line. The
/// browser rounds the ascent and the descent each to the pixel and floors
/// the half-leading above them, so the prototype's baseline is 14px down —
/// every glyph Ferrite drew sat one device pixel under the prototype's, and
/// every mark laid beside text (a dot, a meter, a drawn glyph) had to pick
/// which of the two to agree with.
///
/// **What.** The ascender and descender (`hhea`, and `OS/2`'s typographic
/// pair the face says to use) are rewritten to `BASELINE_ASCENT` and
/// `UNITS - BASELINE_ASCENT`: the same 1300-unit sum, so every line box and
/// content box keeps its height, and gpui's centred baseline lands on the
/// browser's (14.004px at 13/20; 9.08 at the 10/12 badge, the browser's 9).
/// The glyphs themselves are untouched, and so are the files (the
/// prototype loads the same ones).
pub fn browser_baseline(face: &[u8]) -> Vec<u8> {
    let mut face = face.to_vec();
    let be16 = |data: &[u8], at: usize| u16::from_be_bytes([data[at], data[at + 1]]) as usize;
    let be32 = |data: &[u8], at: usize| {
        u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize
    };
    let table = |data: &[u8], tag: &[u8; 4]| -> Option<(usize, usize)> {
        (0..be16(data, 4))
            .map(|i| 12 + 16 * i)
            .find(|record| &data[*record..*record + 4] == tag)
            .map(|record| (be32(data, record + 8), be32(data, record + 12)))
    };
    let (Some((hhea, _)), Some((os2, os2_len))) = (table(&face, b"hhea"), table(&face, b"OS/2"))
    else {
        return face;
    };
    let units = be16(&face, hhea + 4) as i16 as i32 - be16(&face, hhea + 6) as i16 as i32;
    if units != BASELINE_UNITS {
        // Not the face these numbers were measured on: leave it whole.
        return face;
    }
    let ascent = BASELINE_ASCENT as i16;
    let descent = -((BASELINE_UNITS - BASELINE_ASCENT) as i16);
    let mut put = |at: usize, value: i16| face[at..at + 2].copy_from_slice(&value.to_be_bytes());
    put(hhea + 4, ascent);
    put(hhea + 6, descent);
    if os2_len >= 72 {
        put(os2 + 68, ascent);
        put(os2 + 70, descent);
    }
    face
}

/// Geist Mono's ascender plus descender, in font units.
const BASELINE_UNITS: i32 = 1300;
/// The ascender that puts gpui's centred baseline on the browser's at the
/// grid size (`browser_baseline`): `10 + 13 × (958 − 342) / 2000` = 14.004px.
const BASELINE_ASCENT: i32 = 958;

#[cfg(test)]
mod tests {
    use super::*;

    /// The baseline gpui centres at 13/20 lands on the browser's 14px, and
    /// the line keeps its 1300-unit box.
    #[test]
    fn the_face_sets_its_baseline_where_the_browser_does() {
        let face = browser_baseline(crate::FONTS[0]);
        let read = |at: usize| i16::from_be_bytes([face[at], face[at + 1]]) as f32;
        let hhea = (0..u16::from_be_bytes([face[4], face[5]]) as usize)
            .map(|i| 12 + 16 * i)
            .find(|record| &face[*record..*record + 4] == b"hhea")
            .map(|record| {
                u32::from_be_bytes([
                    face[record + 8],
                    face[record + 9],
                    face[record + 10],
                    face[record + 11],
                ]) as usize
            })
            .expect("an hhea table");
        let (ascent, descent) = (read(hhea + 4), -read(hhea + 6));
        assert_eq!(ascent + descent, 1300.);
        let (size, line) = (13., 20.);
        let baseline = (line - (ascent + descent) * size / 1000.) / 2. + ascent * size / 1000.;
        assert!((baseline - 14.).abs() < 0.01, "baseline at {baseline}");
    }
}
