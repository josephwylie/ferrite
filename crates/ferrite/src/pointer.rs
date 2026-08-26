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
use gpui::{rgb, rgba, StyleRefinement};

use crate::theme::{EDGE, EDGE_STRONG, HOVER, PRESSED, RAISED_HOVER, RAISED_PRESSED};

/// The hover styles, named by role. Blanket-implemented: anything styleable
/// and interactive can say what role it plays.
pub trait Pointer: Styled + InteractiveElement + Sized {
    /// A row picked whole, drawn on its container's ground (menu, selector
    /// and nav rows): the HOVER wash, pointer cursor.
    fn hover_row(self) -> Self {
        self.cursor_pointer().hover(row_wash)
    }

    /// A bordered, self-grounded control that does one verb (window
    /// controls, root chip): the EDGE-tone fill, pointer cursor.
    fn hover_control(self) -> Self {
        self.cursor_pointer().hover(control_fill)
    }

    /// A control resting on the opaque RAISED chip ground (keycaps): the
    /// same EDGE lift, precomposed over RAISED — hover replaces the
    /// background, and a translucent fill would let the card behind the
    /// chip bleed through.
    fn hover_raised(self) -> Self {
        self.cursor_pointer().hover(raised_fill)
    }

    /// A Pane as a click-to-focus button (L2/wall cells): the border lifts
    /// EDGE → EDGE_STRONG, pointer cursor. Never a wash — the cell's ground
    /// is the state canvas, and hover speaks border weight so the attention
    /// rings keep color to themselves.
    fn hover_cell(self) -> Self {
        self.cursor_pointer().hover(cell_lift)
    }

    /// A click target already carrying a ground stronger than any wash
    /// (the keyboard-selected row, the accent provider chip): hover adds
    /// nothing the ground doesn't already say, so only the cursor speaks.
    fn hover_carried(self) -> Self {
        self.cursor_pointer()
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
    /// A pressed row: the PRESSED wash (nav rows, rail dots).
    fn press_row(self) -> Self {
        self.active(row_press)
    }

    /// A pressed self-grounded control: the EDGE_STRONG-tone fill.
    fn press_control(self) -> Self {
        self.active(control_press)
    }

    /// A pressed control on the RAISED ground: EDGE_STRONG precomposed,
    /// for the same no-bleed reason as `hover_raised`.
    fn press_raised(self) -> Self {
        self.active(raised_press)
    }
}

impl<E: Pointer + StatefulInteractiveElement> PointerPressed for E {}

// Free functions, not closures, so the refinement each role stores is
// assertable as data — gpui keeps the stored hover style crate-private.

fn row_wash(row: StyleRefinement) -> StyleRefinement {
    row.bg(rgba(HOVER))
}

fn control_fill(control: StyleRefinement) -> StyleRefinement {
    control.bg(rgba(EDGE))
}

fn raised_fill(control: StyleRefinement) -> StyleRefinement {
    control.bg(rgb(RAISED_HOVER))
}

fn cell_lift(cell: StyleRefinement) -> StyleRefinement {
    cell.border_color(rgba(EDGE_STRONG))
}

fn row_press(row: StyleRefinement) -> StyleRefinement {
    row.bg(rgba(PRESSED))
}

fn control_press(control: StyleRefinement) -> StyleRefinement {
    control.bg(rgba(EDGE_STRONG))
}

fn raised_press(control: StyleRefinement) -> StyleRefinement {
    control.bg(rgb(RAISED_PRESSED))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, CursorStyle, Fill};

    fn background(refinement: &StyleRefinement) -> Option<&Fill> {
        refinement.background.as_ref()
    }

    /// The pairing the trait owns: each role's refinement carries exactly
    /// its token — the RAISED variants opaque, so nothing behind a keycap
    /// can bleed through — and every role sets the pointer cursor.
    #[test]
    fn each_role_pairs_its_token_with_the_pointer() {
        let row = row_wash(StyleRefinement::default());
        assert_eq!(background(&row), Some(&Fill::from(rgba(HOVER))));
        assert_eq!(row.border_color, None, "a row hover never touches borders");
        assert_eq!(
            background(&row_press(StyleRefinement::default())),
            Some(&Fill::from(rgba(PRESSED)))
        );

        let control = control_fill(StyleRefinement::default());
        assert_eq!(background(&control), Some(&Fill::from(rgba(EDGE))));
        assert_eq!(
            background(&control_press(StyleRefinement::default())),
            Some(&Fill::from(rgba(EDGE_STRONG)))
        );

        // The keycap faces are precomposed and opaque — `rgb`, never
        // `rgba` — so the card's amber can never tint a hovered keycap.
        assert_eq!(
            background(&raised_fill(StyleRefinement::default())),
            Some(&Fill::from(rgb(RAISED_HOVER)))
        );
        assert_eq!(
            background(&raised_press(StyleRefinement::default())),
            Some(&Fill::from(rgb(RAISED_PRESSED)))
        );

        let cell = cell_lift(StyleRefinement::default());
        assert_eq!(cell.border_color, Some(rgba(EDGE_STRONG).into()));
        assert_eq!(
            background(&cell),
            None,
            "a cell hover is border weight only — the ground is the state canvas"
        );

        for element in [
            div().hover_row(),
            div().hover_control(),
            div().hover_raised(),
            div().hover_cell(),
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
