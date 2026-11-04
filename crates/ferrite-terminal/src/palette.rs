//! A cell's colours from the theme's inks: the sixteen ANSI colours are
//! `theme::TERMINAL_ANSI`, the rest of the 256 are xterm's cube and grey
//! ramp, and a direct colour is itself. The default background is no
//! colour at all: the pane's own ground shows.

use alacritty_terminal::vte::ansi::{Color, NamedColor};
use ferrite_ui::theme;
use gpui::{rgb, Hsla};

/// A foreground colour.
pub(crate) fn foreground(color: Color) -> Hsla {
    rgb(ink(color, theme::INK_STRONG)).into()
}

/// A background colour; `None` for the default, which paints nothing.
pub(crate) fn background(color: Color) -> Option<Hsla> {
    match color {
        Color::Named(NamedColor::Background) => None,
        other => Some(rgb(ink(other, theme::INK_STRONG)).into()),
    }
}

fn ink(color: Color, default: u32) -> u32 {
    match color {
        Color::Spec(rgb) => u32::from(rgb.r) << 16 | u32::from(rgb.g) << 8 | u32::from(rgb.b),
        Color::Indexed(at) => indexed(at),
        Color::Named(named) => match named {
            NamedColor::Foreground | NamedColor::BrightForeground => default,
            NamedColor::DimForeground => theme::INK_SUBTLE,
            NamedColor::Background => theme::TERMINAL_ANSI[0],
            NamedColor::Cursor => theme::TERMINAL_CURSOR,
            other => {
                let at = other as usize;
                match at {
                    0..=15 => theme::TERMINAL_ANSI[at],
                    // Dim black..white are the normal inks.
                    _ => theme::TERMINAL_ANSI[(at - NamedColor::DimBlack as usize).min(7)],
                }
            }
        },
    }
}

/// xterm's 256 colours: the ANSI sixteen, the 6×6×6 cube, 24 greys.
fn indexed(at: u8) -> u32 {
    match at {
        0..=15 => theme::TERMINAL_ANSI[at as usize],
        16..=231 => {
            let at = at - 16;
            let level = |n: u8| if n == 0 { 0 } else { 55 + 40 * u32::from(n) };
            level(at / 36) << 16 | level(at / 6 % 6) << 8 | level(at % 6)
        }
        232..=255 => {
            let grey = 8 + 10 * u32::from(at - 232);
            grey << 16 | grey << 8 | grey
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::Rgb;

    #[test]
    fn the_sixteen_are_the_themes_and_the_cube_and_greys_are_xterms() {
        assert_eq!(ink(Color::Indexed(1), 0), theme::RED);
        assert_eq!(ink(Color::Named(NamedColor::BrightGreen), 0), theme::TERMINAL_ANSI[10]);
        assert_eq!(indexed(16), 0x000000);
        assert_eq!(indexed(231), 0xffffff);
        assert_eq!(indexed(196), 0xff0000, "the cube's red");
        assert_eq!(indexed(232), 0x080808);
        assert_eq!(indexed(255), 0xeeeeee);
    }

    #[test]
    fn a_direct_colour_is_itself_and_the_default_background_paints_nothing() {
        assert_eq!(ink(Color::Spec(Rgb { r: 1, g: 2, b: 3 }), 0), 0x010203);
        assert_eq!(background(Color::Named(NamedColor::Background)), None);
        assert!(background(Color::Indexed(4)).is_some());
        assert_eq!(foreground(Color::Named(NamedColor::Foreground)), rgb(theme::INK_STRONG).into());
    }
}
