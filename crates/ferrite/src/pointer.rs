//! What the pointer means, in one place (#26). Three roles cover every
//! clickable surface, and a render site only says which role a thing is —
//! the trait owns the tokens, the cursor, and the pairing between them.
//! Hover is achromatic: it answers "would a click here land?" and never
//! borrows the accent (keyboard position) or amber/red (attention).
//!
//! Only widget constructors call these (`menu_row`, `keycap`, nav's row
//! frames, the cell shell …) — render sites never write `.hover()`, and
//! gpui's own `debug_assert!(hover_style.is_none())` keeps an element from
//! carrying a second, hand-rolled hover. The transcript body wears the
//! text role (#27): the I-beam and no wash — the wash there is the
//! selection itself, painted per character by select.rs.
//! Constructors keep the state-dependent part (a selected row skips the
//! wash but keeps the cursor, because gpui's hover refinement would
//! *replace* its stronger ground — that skip is `hover_carried`).
//! These are style refinements resolved at paint — no view state, no
//! `cx.notify` loop rides a mouse move.

use gpui::prelude::*;
use gpui::{rgb, StyleRefinement};

use crate::theme::{FILL, FILL_HOVER, HOVER, PRESSED};

/// The hover styles, named by role. Blanket-implemented: anything styleable
/// and interactive can say what role it plays.
pub trait Pointer: Styled + InteractiveElement + Sized {
    /// A row picked whole, drawn on its container's ground (menu, selector
    /// and nav rows): the opaque HOVER face, pointer cursor. Soft's hover is
    /// a solid `#2c2c2c`, not a wash — nothing translucent is layered.
    fn hover_row(self) -> Self {
        self.cursor_pointer().hover(row_wash)
    }

    /// A self-grounded control that does one verb (window controls, root
    /// chip): the HOVER face, pointer cursor. Soft gives it no border.
    fn hover_control(self) -> Self {
        self.cursor_pointer().hover(control_fill)
    }

    /// A control resting on the opaque RAISED chip ground (keycaps): FILL,
    /// one step above RAISED. Every Soft face is opaque, so nothing behind
    /// the chip can bleed through a hover.
    fn hover_raised(self) -> Self {
        self.cursor_pointer().hover(raised_fill)
    }

    /// A click target already carrying the selected FILL (the current
    /// Group row, `.current:hover` in the prototype): hover cannot wash over
    /// a ground stronger than itself, so it steps the ground up instead.
    fn hover_carried(self) -> Self {
        self.cursor_pointer().hover(carried_fill)
    }

    /// Selectable transcript text (#27): the I-beam says characters are
    /// grabbable, and nothing washes — the SELECTION wash is painted per
    /// character by the overlay, not by hover.
    fn hover_text(self) -> Self {
        self.cursor_text()
    }
}

impl<E: Styled + InteractiveElement> Pointer for E {}

/// The pressed shades — the same roles, one step further. A separate trait
/// because gpui's `.active()` tracks the pressed element, which takes
/// element identity: only stateful widgets can wear one.
pub trait PointerPressed: Pointer + StatefulInteractiveElement {
    /// A pressed row: the PRESSED face (nav rows, rail dots).
    fn press_row(self) -> Self {
        self.active(row_press)
    }

    /// A pressed self-grounded control: the PRESSED face.
    fn press_control(self) -> Self {
        self.active(control_press)
    }

    /// A pressed control on the RAISED ground: the PRESSED face, opaque
    /// for the same no-bleed reason as `hover_raised`.
    fn press_raised(self) -> Self {
        self.active(raised_press)
    }
}

impl<E: Pointer + StatefulInteractiveElement> PointerPressed for E {}

// Free functions, not closures, so the refinement each role stores is
// assertable as data — gpui keeps the stored hover style crate-private.

fn row_wash(row: StyleRefinement) -> StyleRefinement {
    row.bg(rgb(HOVER))
}

fn control_fill(control: StyleRefinement) -> StyleRefinement {
    control.bg(rgb(HOVER))
}

fn raised_fill(control: StyleRefinement) -> StyleRefinement {
    control.bg(rgb(FILL))
}

fn carried_fill(row: StyleRefinement) -> StyleRefinement {
    row.bg(rgb(FILL_HOVER))
}

fn row_press(row: StyleRefinement) -> StyleRefinement {
    row.bg(rgb(PRESSED))
}

fn control_press(control: StyleRefinement) -> StyleRefinement {
    control.bg(rgb(PRESSED))
}

fn raised_press(control: StyleRefinement) -> StyleRefinement {
    control.bg(rgb(PRESSED))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, CursorStyle, Fill};

    fn background(refinement: &StyleRefinement) -> Option<&Fill> {
        refinement.background.as_ref()
    }

    /// The pairing the trait owns: each role's refinement carries exactly
    /// its token, and every role sets the pointer cursor. Every Soft face is
    /// opaque — `rgb`, never `rgba` — so nothing behind a hovered surface,
    /// the Decision card's amber included, can tint it.
    #[test]
    fn each_role_pairs_its_token_with_the_pointer() {
        let row = row_wash(StyleRefinement::default());
        assert_eq!(background(&row), Some(&Fill::from(rgb(HOVER))));
        assert_eq!(row.border_color, None, "a row hover never touches borders");
        assert_eq!(
            background(&row_press(StyleRefinement::default())),
            Some(&Fill::from(rgb(PRESSED)))
        );

        let control = control_fill(StyleRefinement::default());
        assert_eq!(background(&control), Some(&Fill::from(rgb(HOVER))));
        assert_eq!(
            background(&control_press(StyleRefinement::default())),
            Some(&Fill::from(rgb(PRESSED)))
        );

        assert_eq!(
            background(&raised_fill(StyleRefinement::default())),
            Some(&Fill::from(rgb(FILL)))
        );
        assert_eq!(
            background(&raised_press(StyleRefinement::default())),
            Some(&Fill::from(rgb(PRESSED)))
        );

        // The selected Group row steps its own ground up rather than
        // washing over it: FILL -> FILL_HOVER, the prototype's
        // `.current:hover`.
        assert_eq!(
            background(&carried_fill(StyleRefinement::default())),
            Some(&Fill::from(rgb(FILL_HOVER)))
        );

        for element in [
            div().hover_row(),
            div().hover_control(),
            div().hover_raised(),
            div().hover_carried(),
        ] {
            let mut element = element;
            assert_eq!(
                element.style().mouse_cursor,
                Some(CursorStyle::PointingHand),
                "every role advertises the click with the pointer cursor"
            );
        }

        // The text role speaks the I-beam, not the pointer: characters are
        // grabbable, nothing is a button (#27).
        let mut text = div().hover_text();
        assert_eq!(text.style().mouse_cursor, Some(CursorStyle::IBeam));
    }
}
