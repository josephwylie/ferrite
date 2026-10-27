// Ferrite's shared UI: what the Cockpit and the Pane both draw with.
pub mod components;
#[cfg(any(test, feature = "test-support"))]
pub mod face;
pub mod frame;
pub mod icons;
pub mod keymap;
pub mod loops_overlay;
pub mod menu;
pub mod motion;
pub mod platform_text;
pub mod pointer;
mod scrollbar;
pub mod sheet;
pub mod theme;

use gpui::App;

/// The bundled faces, compiled in: Geist Mono for the transcript and code,
/// Geist for the chrome (theme rule 1). gpui has no variation-axis support,
/// so each weight is its own static file. A face's weights share its
/// typographic family name (`theme::FONT_CODE` = Geist Mono,
/// `theme::FONT_UI` = Geist), and CoreText / DirectWrite resolve the right
/// one from `.font_weight(..)` — never reach a weight by family name.
/// Geist Mono comes first: the baseline correction is measured on it.
pub static FONTS: [&[u8]; 10] = [
    include_bytes!("../assets/fonts/GeistMono.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Italic.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Medium.ttf"),
    include_bytes!("../assets/fonts/GeistMono-SemiBold.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Bold.ttf"),
    include_bytes!("../assets/fonts/Geist.ttf"),
    include_bytes!("../assets/fonts/Geist-Italic.ttf"),
    include_bytes!("../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Geist-Bold.ttf"),
];

/// Registers the bundled faces. Call it before anything lays out text:
/// `add_fonts` returns a Result, and a discarded one fails silently — you get
/// the system font and no explanation.
pub fn register_fonts(cx: &App) {
    cx.text_system()
        .add_fonts(
            FONTS
                .iter()
                .map(|face| std::borrow::Cow::Owned(platform_text::browser_baseline(face)))
                .collect(),
        )
        .expect("the bundled faces load");
}
