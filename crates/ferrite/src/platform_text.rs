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
/// Idempotent; safe to call before `App` exists.
pub fn disable_font_smoothing() {
    #[cfg(target_os = "macos")]
    mac::set_app_font_smoothing(0);
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

    /// `AppleFontSmoothing = value` in the current application's domain.
    pub(super) fn set_app_font_smoothing(value: i32) {
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
            let number = CFNumberCreate(
                std::ptr::null(),
                NUMBER_SINT32,
                (&value as *const i32).cast::<c_void>(),
            );
            if !number.is_null() {
                CFPreferencesSetAppValue(key, number, kCFPreferencesCurrentApplication);
                CFRelease(number);
            }
            CFRelease(key);
        }
    }
}
