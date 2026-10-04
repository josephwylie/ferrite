//! What the pointer means, in one place (#26). Three roles cover every
//! clickable surface, and a render site only says which role a thing is —
//! the trait owns the tokens, the cursor, and the pairing between them.
//! Hover is achromatic: it answers "would a click here land?" and never
//! borrows the accent (keyboard position) or amber/red (attention).
//!
//! Only widget constructors call these (`menu_row`, `keycap`, nav's row
//! frames, the cell shell …) — render sites never write a hover refinement
//! of their own (a guard test below fails on one anywhere else in `src`).
//! The transcript body wears the text role (#27): the I-beam and no wash —
//! the wash there is the selection itself, painted per character by
//! select.rs.
//!
//! **One hover blend** (rule 2.10.2). Every role paints its ground through
//! `motion::hover_blend` over 150ms (`MOTION_HOVER_FADE_MS`), driven by an
//! `on_hover` listener on the same element, under a stable `key` the caller
//! derives from the element's id (unique across the window: add the Pane or
//! Thread when the id alone repeats). The role owns the element's ground:
//! the caller sets none after it. A press (`press_*`, gpui's `.active()`)
//! lands at once over the blend, and a keyboard-armed row (the menu cursor,
//! a selected option) takes its `FILL` on the same frame — only the pointer
//! half of its ladder blends. Kit buttons take the same blend through
//! `components::faded_button`, whose variant (`button_variant`) holds hover
//! equal to rest so the kit's own hover never snaps over it.
//!
//! **One ladder on every ground** (theme rules 3 and 4): rest nothing;
//! hover `paint::HOVER`; the cursor or a selected row `paint::SELECTION`;
//! that row under the pointer `paint::SELECTION_HOVER`; press
//! `paint::PRESS`. On glass (macOS) every face is a white overlay, so it
//! reads on chrome, the plane, a band or a float alike; elsewhere each is
//! its opaque grey.

use gpui::component::button::ButtonCustomVariant;
use gpui::prelude::*;
use gpui::{rgba, App, Hsla, SharedString, StyleRefinement};

use crate::motion;
use crate::theme::{paint, HAIRLINE_STRONG, TRANSPARENT};

/// The hover styles, named by role. Blanket-implemented: anything styleable
/// and interactive can say what role it plays. `key` names the element's
/// blend (see the module doc); the element must carry an id, since gpui
/// delivers hover only to an element it keeps state for.
pub trait Pointer: Styled + InteractiveElement + Sized {
    /// A row picked whole, drawn on its container's ground (menu, selector
    /// and nav rows): nothing at rest, the hover face under the pointer,
    /// pointer cursor.
    fn hover_row(self, key: impl Into<SharedString>) -> Self {
        blended(
            self.cursor_pointer(),
            key.into(),
            rgba(TRANSPARENT).into(),
            row_face(),
        )
    }

    /// A self-grounded control that does one verb (window controls, root
    /// chip): the hover face, pointer cursor, no border.
    fn hover_control(self, key: impl Into<SharedString>) -> Self {
        blended(
            self.cursor_pointer(),
            key.into(),
            rgba(TRANSPARENT).into(),
            control_face(),
        )
    }

    /// A control on a float or a band (keycaps, menu rows, options): the
    /// same hover face as every other ground — one ladder everywhere.
    fn hover_raised(self, key: impl Into<SharedString>) -> Self {
        blended(
            self.cursor_pointer(),
            key.into(),
            rgba(TRANSPARENT).into(),
            raised_face(),
        )
    }

    /// A click target already carrying the selection (the current Group
    /// row, the menu cursor): hover cannot wash over a ground stronger than
    /// itself, so it steps the ground up instead, `SELECTION` →
    /// `SELECTION_HOVER`. The selection is painted here, at once: arming a
    /// row is a keyboard change.
    fn hover_carried(self, key: impl Into<SharedString>) -> Self {
        let key = key.into();
        let ground = motion::hover_blend(&key, paint::SELECTION.into(), carried_face());
        let mut element = self.cursor_pointer().bg(ground);
        listen(&mut element, key);
        element
    }

    /// A surface whose resting edge is the hairline (a Pane): under the
    /// pointer the edge steps up to `HAIRLINE_STRONG`, saying a click lands
    /// here. No cursor change — the surface is a focus target, not a
    /// button. Apply it only while the edge is the resting hairline: the
    /// hover refinement would otherwise replace a state colour.
    #[allow(dead_code)]
    fn hover_edge(self) -> Self {
        self.hover(edge_lift)
    }

    /// Selectable transcript text (#27): the I-beam says characters are
    /// grabbable, and nothing washes — the SELECTION wash is painted per
    /// character by the overlay, not by hover.
    fn hover_text(self) -> Self {
        self.cursor_text()
    }
}

/// Paint `rest → hover` at `key`'s blend progress: nothing is painted at
/// rest (the element keeps its own resting ground), the blended face while
/// the blend is off zero.
fn blended<E: Styled + InteractiveElement>(
    mut element: E,
    key: SharedString,
    rest: Hsla,
    hover: Hsla,
) -> E {
    let t = motion::hover_t(&key);
    if t > 0.0 {
        element = element.bg(motion::mix(rest, hover, t));
    }
    listen(&mut element, key);
    element
}

fn listen<E: InteractiveElement>(element: &mut E, key: SharedString) {
    element
        .interactivity()
        .on_hover(motion::hover_listener(key));
}

impl<E: Styled + InteractiveElement> Pointer for E {}

/// The pressed shades — the same roles, one step further. A separate trait
/// because gpui's `.active()` tracks the pressed element, which takes
/// element identity: only stateful widgets can wear one.
pub trait PointerPressed: Pointer + StatefulInteractiveElement {
    /// A pressed row: the press face (nav rows, rail dots).
    fn press_row(self) -> Self {
        self.active(row_press)
    }

    /// A pressed self-grounded control: the press face.
    fn press_control(self) -> Self {
        self.active(control_press)
    }

    /// A pressed control on a float or a band: the press face.
    fn press_raised(self) -> Self {
        self.active(raised_press)
    }
}

impl<E: Pointer + StatefulInteractiveElement> PointerPressed for E {}

/// The blend key for an element: its id, prefixed so it never collides with
/// a hand-named blend. Ids that repeat across the window (per-Pane rows)
/// carry the Pane or Thread in the id itself.
pub fn hover_key(id: &gpui::ElementId) -> SharedString {
    format!("hover:{id}").into()
}

/// A kit `Button`'s variant for the one hover blend: `ground` is the
/// blended face (`motion::hover_blend`), and the kit's hover is held equal to
/// it so the kit's own snapping hover never lands over the blend; the press
/// stays the kit's instant `.active()`.
pub fn button_variant(ground: Hsla, ink: Hsla, press: Hsla, cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(ground)
        .foreground(ink)
        .hover(ground)
        .active(press)
}

// Each role's hover face, named so the tokens are assertable as data.

fn row_face() -> Hsla {
    paint::HOVER.into()
}

fn control_face() -> Hsla {
    paint::HOVER.into()
}

fn raised_face() -> Hsla {
    paint::HOVER.into()
}

fn carried_face() -> Hsla {
    paint::SELECTION_HOVER.into()
}

#[allow(dead_code)]
fn edge_lift(surface: StyleRefinement) -> StyleRefinement {
    surface.border_color(rgba(HAIRLINE_STRONG))
}

fn row_press(row: StyleRefinement) -> StyleRefinement {
    row.bg(paint::PRESS)
}

fn control_press(control: StyleRefinement) -> StyleRefinement {
    control.bg(paint::PRESS)
}

fn raised_press(control: StyleRefinement) -> StyleRefinement {
    control.bg(paint::PRESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, CursorStyle, Fill};

    fn background(refinement: &StyleRefinement) -> Option<&Fill> {
        refinement.background.as_ref()
    }

    /// The pairing the trait owns: each role's face is exactly its paint
    /// (theme rule 4: a white overlay on glass, the opaque grey elsewhere),
    /// and every role sets the pointer cursor. One hover face on every
    /// ground, one step under the selection, so a hovered row never reads
    /// as the armed one. (The old opaque-only rule, `rgb` never `rgba`, is
    /// retired with the glass frame.)
    #[test]
    fn each_role_pairs_its_token_with_the_pointer() {
        assert_eq!(row_face(), Hsla::from(paint::HOVER));
        assert_eq!(control_face(), Hsla::from(paint::HOVER));
        assert_eq!(raised_face(), Hsla::from(paint::HOVER));
        assert_ne!(raised_face(), Hsla::from(paint::SELECTION));
        for press in [row_press, control_press, raised_press] {
            assert_eq!(
                background(&press(StyleRefinement::default())),
                Some(&Fill::from(paint::PRESS))
            );
        }

        // The selected Group row steps its own ground up rather than
        // washing over it: SELECTION -> SELECTION_HOVER.
        assert_eq!(carried_face(), Hsla::from(paint::SELECTION_HOVER));

        for element in [
            div().id("row").hover_row("pointer-test-row"),
            div().id("control").hover_control("pointer-test-control"),
            div().id("raised").hover_raised("pointer-test-raised"),
            div().id("carried").hover_carried("pointer-test-carried"),
        ] {
            let mut element = element;
            assert_eq!(
                element.style().mouse_cursor,
                Some(CursorStyle::PointingHand),
                "every role advertises the click with the pointer cursor"
            );
        }

        // At rest a row paints nothing of its own, and the carried role
        // paints its FILL at once: arming a row is a keyboard change.
        let mut row = div().id("rest").hover_row("pointer-test-rest");
        assert_eq!(background(row.style()), None);
        let mut carried = div().id("armed").hover_carried("pointer-test-armed");
        assert_eq!(
            background(carried.style()),
            Some(&Fill::from(Hsla::from(paint::SELECTION)))
        );

        // The text role speaks the I-beam, not the pointer: characters are
        // grabbable, nothing is a button (#27).
        let mut text = div().hover_text();
        assert_eq!(text.style().mouse_cursor, Some(CursorStyle::IBeam));
    }

    /// The edge role lifts only the border, to the strong hairline, and
    /// leaves the cursor alone: a Pane is a focus target, not a button.
    #[test]
    fn the_edge_role_lifts_the_border_and_keeps_the_cursor() {
        let edge = edge_lift(StyleRefinement::default());
        assert_eq!(edge.border_color, Some(rgba(HAIRLINE_STRONG).into()));
        assert_eq!(background(&edge), None);
        let mut surface = div().hover_edge();
        assert_eq!(surface.style().mouse_cursor, None);
    }

    /// One hover blend (rule 2.10.2): no render site writes its own hover
    /// refinement or a kit variant's snapping hover. Every hover goes
    /// through the roles here (or `components::faded_button`), so every
    /// hover in the window blends over 150ms.
    #[test]
    fn no_render_site_writes_its_own_hover() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src];
        let mut offenders = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                if !name.ends_with(".rs") || name == "pointer.rs" || name == "motion.rs" {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (line, content) in text.lines().enumerate() {
                    if content.contains(".hover(") {
                        offenders.push(format!("{}:{}", path.display(), line + 1));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "hover outside the shared roles: {offenders:#?}"
        );
    }
}
