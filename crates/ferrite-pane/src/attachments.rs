//! The files a draft carries, over the Composer (SPEC §5.5): an image as a
//! thumbnail card (Claude's), any other file as a shelf chip —
//! `SHELF_CHIP_H` high at `R_CONTROL` on `paint::RAISED`, Geist `FS_META`,
//! the file mark and the name in `LINK_INK`. The card is the sent image's
//! too (`thumbnail`). This module owns presentation and
//! opening (in the Pane's preview, the Panel's Preview tab: `Preview::open`); callers
//! only supply paths and, for a draft, a removal callback. The prompt codec owns persistence. A draft's review comments
//! (ADR 0021) ride the same shelf as one more chip. A draft's replies (a
//! selection's Reply, `crate::quote`) are chips of their own in
//! `composer::citations`; a sent prompt draws its quotes as Quote rows over
//! its bubble (`quote_row`).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Instant,
};

use gpui::{prelude::*, px, App, ElementId, IntoElement, SharedString, Window};

use crate::preview::Preview;
use ferrite_core::attachment::{Failure, Readiness};
use ferrite_core::browser_annotations::Annotation;
use ferrite_core::replies::Reply;

/// Nothing to install now: delivered files draw in the float grammar rather
/// than on the kit's stock attachment cards, so the kit's own tokens are no
/// longer captured. Kept for the theme's init order (`init_components`).
pub fn init(_cx: &mut App) {}

type Remove = Rc<dyn Fn(&PathBuf, &mut Window, &mut App)>;
type Retry = Rc<dyn Fn(&PathBuf, &mut Window, &mut App)>;
type DropReview = Rc<dyn Fn(&mut Window, &mut App)>;
type RemoveAnnotation = Rc<dyn Fn(&str, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Attachments {
    id: ElementId,
    files: Vec<PathBuf>,
    preview: Preview,
    on_remove: Option<Remove>,
    review: usize,
    on_drop_review: Option<DropReview>,
    island: bool,
    annotations: Vec<Annotation>,
    on_remove_annotation: Option<RemoveAnnotation>,
    added: HashSet<PathBuf>,
    closing: HashMap<PathBuf, Instant>,
    readiness: HashMap<PathBuf, Readiness>,
    on_retry: Option<Retry>,
    local: HashSet<PathBuf>,
    uploaded: HashMap<PathBuf, PathBuf>,
}

impl Attachments {
    pub fn new(id: impl Into<ElementId>, files: Vec<PathBuf>, preview: &Preview) -> Self {
        Self {
            id: id.into(),
            files,
            preview: preview.clone(),
            on_remove: None,
            review: 0,
            on_drop_review: None,
            island: false,
            annotations: Vec::new(),
            on_remove_annotation: None,
            added: HashSet::new(),
            closing: HashMap::new(),
            readiness: HashMap::new(),
            on_retry: None,
            local: HashSet::new(),
            uploaded: HashMap::new(),
        }
    }

    /// What a send would make of each file: a failed one shows why in its
    /// own place, and a retry looks at it again.
    pub fn readiness(mut self, readiness: HashMap<PathBuf, Readiness>) -> Self {
        self.readiness = readiness;
        self
    }

    pub fn sources(mut self, local: HashSet<PathBuf>, uploaded: HashMap<PathBuf, PathBuf>) -> Self {
        self.local = local;
        self.uploaded = uploaded;
        self
    }

    pub fn on_retry(
        mut self,
        callback: impl Fn(&PathBuf, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_retry = Some(Rc::new(callback));
        self
    }

    /// The files just added to a draft: they grow in (`motion::chip_in`).
    pub fn added(mut self, added: HashSet<PathBuf>) -> Self {
        self.added = added;
        self
    }

    /// The files taken off the draft a moment ago and still among `files`,
    /// each with when it went: they close their places with no controls
    /// (`motion::slip_out`).
    pub fn closing(mut self, closing: HashMap<PathBuf, Instant>) -> Self {
        self.closing = closing;
        self
    }

    pub fn annotations(mut self, annotations: Vec<Annotation>) -> Self {
        self.annotations = annotations;
        self
    }

    pub fn on_remove_annotation(
        mut self,
        callback: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_remove_annotation = Some(Rc::new(callback));
        self
    }

    /// Pending attachments as chips on the shelf above the prompt.
    pub fn in_island(mut self) -> Self {
        self.island = true;
        self
    }

    pub fn on_remove(
        mut self,
        callback: impl Fn(&PathBuf, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_remove = Some(Rc::new(callback));
        self
    }

    /// `comments` review comments wait with the files: one chip for all.
    pub fn review(mut self, comments: usize) -> Self {
        self.review = comments;
        self
    }

    /// The review chip's ×: every gathered comment goes.
    pub fn on_drop_review(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_drop_review = Some(Rc::new(callback));
        self
    }
}

/// The draft's files: each image as a thumbnail card (`image_card`) in a
/// wrapping row, then the other files as shelf chips, wrapping
/// (`SHELF_CHIP_H` on `paint::RAISED`) — the `FILE` mark, the name in
/// `LINK_INK` cut at `ATTACH_CHIP_MAX_W` (a file name is machine text),
/// and, on a draft, a quiet `×` that removes it (a button). The buttons
/// sit in the `PromptAttachment` key context, so their Enter never sends.
/// A click on a card or a chip opens the image in the lightbox, among the
/// tray's images, or the file in the Pane's preview.
/// A file `added` to a draft's tray while it was drawn grows in
/// (`motion::chip_in`), keyed on its path.
fn grown(added: &HashSet<PathBuf>, path: &Path, element: impl IntoElement) -> gpui::AnyElement {
    if !added.contains(path) {
        return element.into_any_element();
    }
    let id = SharedString::from(format!("chip-in-{}", path.display()));
    ferrite_ui::motion::chip_in(id, gpui::div().child(element)).into_any_element()
}

/// A file taken off the draft at `left`, closing its place.
/// `gap` is the row's, which the closing chip gives back as it goes. Its
/// id is the file and the moment it went, not its place, which moves as
/// the ghosts before it expire.
fn slipping(path: &Path, left: Instant, gap: f32, ghost: impl IntoElement) -> gpui::AnyElement {
    let id = SharedString::from(format!("attachment-out-{}-{left:?}", path.display()));
    ferrite_ui::motion::slip_out(id, left, ferrite_ui::motion::CHIP_OUT, gap, ghost)
        .into_any_element()
}

/// An image card with no controls, for the moment it closes.
fn ghost_card(index: usize, path: &Path, preview: &Preview, local: bool) -> gpui::Div {
    use ferrite_ui::theme;
    gpui::div()
        .flex_shrink_0()
        .size(px(theme::COMPOSER_THUMB))
        .rounded(px(theme::R_CARD))
        .overflow_hidden()
        .child(if local || !preview.remote_files() {
            thumbnail(
                ("tray-image", index),
                path,
                (theme::COMPOSER_THUMB, theme::COMPOSER_THUMB),
            )
        } else {
            remote_thumbnail(
                ("tray-image", index),
                path,
                preview,
                (theme::COMPOSER_THUMB, theme::COMPOSER_THUMB),
            )
        })
}

fn chips(attachments: Attachments) -> gpui::AnyElement {
    use ferrite_ui::pointer::Pointer as _;
    use ferrite_ui::theme;
    use gpui::{div, rgb, SharedString};
    let Attachments {
        id,
        files,
        preview,
        on_remove,
        review,
        on_drop_review,
        island,
        annotations,
        on_remove_annotation,
        added,
        closing,
        readiness,
        on_retry,
        local,
        uploaded,
    } = attachments;
    let (images, others): (Vec<_>, Vec<_>) = files
        .into_iter()
        .enumerate()
        .partition(|(_, path)| crate::image_viewer::is_image(path));
    let gallery: Arc<[PathBuf]> = images
        .iter()
        .map(|(_, path)| uploaded.get(path).unwrap_or(path).clone())
        .collect();
    let local_gallery: Arc<[PathBuf]> = images
        .iter()
        .filter(|(_, path)| {
            !preview.remote_files() || (local.contains(path) && !uploaded.contains_key(path))
        })
        .map(|(_, path)| path.clone())
        .collect();
    let mut cards: Vec<_> = images
        .iter()
        .map(|(index, path)| {
            if let Some(left) = closing.get(path) {
                return slipping(
                    path,
                    *left,
                    ferrite_ui::theme::SPACE_2,
                    ghost_card(*index, path, &preview, local.contains(path)),
                );
            }
            grown(
                &added,
                path,
                image_card(
                    *index,
                    path,
                    if local.contains(path) && !uploaded.contains_key(path) {
                        &local_gallery
                    } else {
                        &gallery
                    },
                    &preview,
                    on_remove.clone(),
                    readiness.get(path),
                    on_retry.clone(),
                    local.contains(path),
                    uploaded.get(path),
                ),
            )
        })
        .collect();
    cards.extend(
        annotations
            .into_iter()
            .enumerate()
            .map(|(index, annotation)| {
                annotation_card(index, annotation, &preview, on_remove_annotation.clone())
            }),
    );
    let others_empty = others.is_empty();
    let chips = others.into_iter().map(|(index, path)| {
        let left = closing.get(&path).copied();
        let live = left.is_none();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let host = preview.clone();
        let open = uploaded.get(&path).unwrap_or(&path).clone();
        let open_local = local.contains(&path) && !uploaded.contains_key(&path);
        let explanation = readiness.get(&path).and_then(Readiness::explanation);
        let chip = div()
            .id(("attachment", index))
            .debug_selector(move || format!("tray-file-{index}"))
            .when_some(explanation.filter(|_| live), |chip, why| {
                chip.tooltip(ferrite_ui::menu::tooltip(why))
            })
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::CH))
            .h(px(theme::ATTACH_CHIP_H))
            .max_w(px(theme::ATTACH_CHIP_MAX_W))
            .min_w_0()
            .px(px(theme::SPACE_2))
            .rounded(px(theme::R_CONTROL))
            .bg(theme::paint::RAISED)
            .when(live, |chip| {
                chip.cursor_pointer()
                    .hover_carried(format!("attachment-{index}-{}", open.display()))
            })
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_META))
            .line_height(px(theme::LH_META))
            .font_weight(theme::W_BODY)
            .text_color(rgb(theme::LINK_INK))
            .whitespace_nowrap()
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(theme::ATTACH_THUMB))
                    .child(ferrite_ui::icons::icon(
                        ferrite_ui::icons::FILE,
                        theme::ATTACH_THUMB,
                        theme::LINK_INK,
                    )),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(name.clone())),
            )
            .children(
                readiness
                    .get(&path)
                    .and_then(|state| status_label(("attachment-status", index), state)),
            )
            .when_some(
                retry_of(readiness.get(&path), on_retry.clone()),
                |chip, retry| {
                    let retried = path.clone();
                    let button = retry_button(("retry-attachment", index))
                        .debug_selector(move || format!("tray-file-{index}-retry"))
                        .tab_stop(live)
                        .accessibility_label(format!("Retry {name}"));
                    chip.child(if live {
                        button.on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            retry(&retried, window, cx);
                        })
                    } else {
                        button
                    })
                },
            )
            .when_some(on_remove.clone(), |chip, remove| {
                let removed = path.clone();
                let button = remove_button(("remove-attachment", index))
                    .tab_stop(live)
                    .accessibility_label(format!("Remove {name}"));
                chip.child(if live {
                    button
                        .debug_selector(move || format!("tray-file-{index}-remove"))
                        .tooltip_builder(ferrite_ui::menu::tooltip(format!(
                            "Remove {}",
                            path.display()
                        )))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            remove(&removed, window, cx);
                        })
                } else {
                    button
                })
            })
            .when(live, |chip| {
                chip.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if open_local {
                        host.open_local(open.clone(), None, &[], window, cx);
                    } else {
                        host.open(open.clone(), None, &[], window, cx);
                    }
                })
            });
        match left {
            Some(left) => slipping(&path, left, theme::CH, chip),
            None => grown(&added, &path, chip),
        }
    });
    let review = (review > 0).then(|| review_chip(review, on_drop_review));
    let has_chips = !others_empty || review.is_some();
    let chips = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(theme::CH))
        .min_w_0()
        .max_w_full()
        .children(chips)
        .children(review);
    let has_cards = !cards.is_empty();
    let cards = div()
        .flex()
        .flex_wrap()
        .gap(px(theme::SPACE_2))
        .min_w_0()
        .max_w_full()
        .children(cards);
    div()
        .id(id)
        .when(island, |shelf| {
            shelf.debug_selector(|| "attachment-island-content".into())
        })
        .flex()
        .flex_col()
        .gap(px(theme::CH))
        .min_w_0()
        .max_w_full()
        .when(has_cards, |shelf| shelf.child(cards))
        .when(has_chips, |shelf| shelf.child(chips))
        .into_any_element()
}

/// A Browser annotation stays distinct from an ordinary file or source Review.
/// Its captured URL and comment remain visible even when its page has gone.
fn annotation_card(
    index: usize,
    annotation: Annotation,
    preview: &Preview,
    remove: Option<RemoveAnnotation>,
) -> gpui::AnyElement {
    use ferrite_ui::{pointer::Pointer as _, theme};
    let image = annotation.original().filter(|path| path.is_file()).map(Path::to_path_buf)
        .or_else(|| annotation.cached_image());
    let host = preview.clone();
    let id = annotation.id().to_owned();
    let comment = annotation.comment().to_owned();
    let url = annotation.capture().url.clone();
    gpui::div()
        .id(SharedString::from(format!("browser-annotation-{id}")))
        .debug_selector(|| "browser-annotation-attachment".into())
        .flex()
        .items_center()
        .gap(px(theme::SPACE_2))
        .min_w_0()
        .max_w(px(theme::ATTACH_CHIP_MAX_W))
        .p(px(theme::SPACE_2))
        .rounded(px(theme::R_CARD))
        .bg(theme::paint::RAISED)
        .child(
            ferrite_ui::components::button(("review-browser-annotation", index))
                .debug_selector(move || format!("review-browser-annotation-{index}"))
                .tab_stop(true)
                .key_context("PromptAttachment")
                .accessibility_label("Review browser annotation")
                .p_0()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_2))
                .min_w_0()
                .cursor_pointer()
                .hover_carried(format!("browser-annotation-{id}"))
                .when_some(image, |card, image| {
                    card.child(thumbnail(
                        ("browser-annotation-image", index),
                        &image,
                        (64.0, 48.0),
                    ))
                })
                .child(
                    gpui::div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .font_family(theme::FONT_UI)
                        .text_size(px(theme::FS_META))
                        .text_color(gpui::rgb(theme::INK_SUBTLE))
                        .child("Browser annotation")
                        .child(
                            gpui::div()
                                .truncate()
                                .text_color(gpui::rgb(theme::INK_STRONG))
                                .child(comment),
                        )
                        .child(gpui::div().truncate().child(url)),
                )
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    host.review_annotation(annotation.clone(), window);
                }),
        )
        .when_some(remove, |card, remove| {
            card.child(
                remove_button(("remove-browser-annotation", index))
                    .debug_selector(move || format!("remove-browser-annotation-{index}"))
                    .accessibility_label("Remove browser annotation")
                    .tooltip_builder(ferrite_ui::menu::tooltip("Remove browser annotation"))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        remove(&id, window, cx);
                    }),
            )
        })
        .into_any_element()
}

/// An image waiting in the tray as Claude shows one: the picture covering
/// a `COMPOSER_THUMB` card (`thumbnail`), a button — a tab stop,
/// Enter/Space — that opens it in the lightbox among the tray's images;
/// on a draft, its `×` in the corner — shown under the pointer or on
/// keyboard focus — removes it.
fn image_card(
    index: usize,
    path: &Path,
    gallery: &Arc<[PathBuf]>,
    preview: &Preview,
    on_remove: Option<Remove>,
    readiness: Option<&Readiness>,
    on_retry: Option<Retry>,
    local: bool,
    uploaded: Option<&PathBuf>,
) -> gpui::AnyElement {
    use ferrite_ui::theme;
    use gpui::div;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let group = SharedString::from(format!("tray-image-{index}"));
    let host = preview.clone();
    let open = uploaded.map_or_else(|| path.to_path_buf(), Clone::clone);
    let open_local = local && uploaded.is_none();
    let gallery = gallery.clone();
    let retry = retry_of(readiness, on_retry);
    let repair_on_open = matches!(
        readiness,
        Some(Readiness::Failed(Failure::Missing | Failure::Gone | Failure::Unreadable(_)))
    );
    let repair = retry.clone();
    let repair_path = path.to_path_buf();
    let image = if open_local || !preview.remote_files() {
        thumbnail(
            ("tray-image", index),
            &open,
            (theme::COMPOSER_THUMB, theme::COMPOSER_THUMB),
        )
    } else {
        remote_thumbnail(
            ("tray-image", index),
            &open,
            preview,
            (theme::COMPOSER_THUMB, theme::COMPOSER_THUMB),
        )
    };
    let card = ferrite_ui::components::button(("preview-attachment", index))
        .tab_stop(true)
        .p_0()
        .size(px(theme::COMPOSER_THUMB))
        .rounded(px(theme::R_CARD))
        .key_context("PromptAttachment")
        .accessibility_label(if repair_on_open {
            format!("Retry {name}")
        } else {
            format!("Preview {name}")
        })
        .when_some(readiness.and_then(Readiness::explanation), |card, why| {
            card.tooltip_builder(ferrite_ui::menu::tooltip(why))
        })
        .child(image)
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            if repair_on_open {
                if let Some(repair) = &repair {
                    repair(&repair_path, window, cx);
                }
            } else if open_local {
                host.open_local(open.clone(), None, &gallery, window, cx);
            } else {
                host.open(open.clone(), None, &gallery, window, cx);
            }
        });
    let remove = on_remove.map(|remove| {
        let removed = path.to_path_buf();
        tile_remove(
            format!("tray-image-{index}-remove"),
            ("remove-attachment", index),
            group.clone(),
            format!("Remove {name}"),
            format!("Remove {}", path.display()),
            move |window, cx| remove(&removed, window, cx),
        )
    });
    div()
        .id(("tray-image-card", index))
        .debug_selector(move || format!("tray-image-{index}"))
        .group(group)
        .relative()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(theme::SPACE_1))
        .child(card)
        .children(readiness.and_then(|state| status_label(("attachment-status", index), state)))
        .when_some(retry, |card, retry| {
            let retried = path.to_path_buf();
            card.child(
                retry_button(("retry-attachment", index))
                    .debug_selector(move || format!("tray-image-{index}-retry"))
                    .accessibility_label(format!("Retry {name}"))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        retry(&retried, window, cx);
                    }),
            )
        })
        .children(remove)
        .into_any_element()
}

pub(crate) fn remote_thumbnail(
    id: impl Into<ElementId>,
    path: &Path,
    preview: &Preview,
    (width, height): (f32, f32),
) -> gpui::Stateful<gpui::Div> {
    if let Some(local) = preview.attachment_local(path) {
        return thumbnail(id, &local, (width, height));
    }
    use ferrite_ui::theme;
    let label = if preview.attachment_failed(path) {
        "Preview unavailable"
    } else if ferrite_core::attachment_transfer::is_retained_path(path) {
        "Loading preview"
    } else {
        "Remote image"
    };
    gpui::div()
        .id(id)
        .debug_selector(|| "attachment-preview-pending".into())
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .w(px(width))
        .h(px(height))
        .rounded(px(theme::R_CARD))
        .border_1()
        .border_color(theme::paint::LINE)
        .bg(theme::paint::RAISED)
        .text_size(px(theme::FS_LABEL))
        .text_color(gpui::rgb(theme::INK_SUBTLE))
        .child(label)
}

/// An image as a card, in the tray and over a sent prompt (Claude's): the
/// picture covering `width` × `height` at `R_CARD` in a 1px `LINE` ring,
/// its caption (`name · W×H · 41 KB`, `pane::image_caption`) its tooltip.
/// The caller wires what a click does.
pub(crate) fn thumbnail(
    id: impl Into<ElementId>,
    path: &Path,
    (width, height): (f32, f32),
) -> gpui::Stateful<gpui::Div> {
    use ferrite_ui::theme;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let facts = crate::pane::image_facts(path);
    let caption = crate::pane::image_caption(&name, facts);
    // A file that cannot be read is Codex's failed tile: a red hairline and
    // a word, not a blank card. A readable one is drawn, whether or not its
    // header says how big it is.
    if facts.is_none() {
        return failed_tile(id, (width, height), "Missing", false);
    }
    gpui::div()
        .id(id)
        .flex_shrink_0()
        .rounded(px(theme::R_CARD))
        .border_1()
        .border_color(theme::paint::LINE)
        .overflow_hidden()
        .tooltip(ferrite_ui::menu::tooltip(caption))
        .child(
            gpui::img(path.to_path_buf())
                .w(px(width))
                .h(px(height))
                .rounded(px(theme::R_CARD - theme::HAIRLINE_W))
                .object_fit(gpui::ObjectFit::Cover),
        )
}

/// An image's place when it cannot be shown: the same size, so the tray does
/// not move, saying why, and `Retry` under it when a look again may help.
fn failed_tile(
    id: impl Into<ElementId>,
    (width, height): (f32, f32),
    label: &'static str,
    retry: bool,
) -> gpui::Stateful<gpui::Div> {
    use ferrite_ui::theme;
    gpui::div()
        .id(id)
        .debug_selector(|| "attachment-failed".into())
        .flex_shrink_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(theme::SPACE_1))
        .w(px(width))
        .h(px(height))
        .rounded(px(theme::R_CARD))
        .border_1()
        .border_color(ferrite_ui::theme::Paint::solid(theme::RED))
        .overflow_hidden()
        .text_size(px(theme::FS_LABEL))
        .text_color(gpui::rgb(theme::RED))
        .child(label)
        .when(retry, |tile| {
            tile.child(
                gpui::div()
                    .text_color(gpui::rgb(theme::INK_SUBTLE))
                    .child("Retry"),
            )
        })
}

/// A quoted passage as Claude draws one over the sent prompt's bubble: the
/// word Quote in `INK_SUBTLE`, then the passage cut at `QUOTE_ROW_LINES`
/// lines, behind a `QUOTE_RULE_W` `LINE` rule (Markdown's quote rule), and
/// under it the words the operator replied with, when there are any.
pub fn quote_row(
    index: usize,
    reply: Reply,
    source: Option<QuoteSource>,
) -> gpui::Stateful<gpui::Div> {
    use ferrite_ui::theme;
    use gpui::{div, rgb, SharedString};
    div()
        .id(("quote", index))
        .debug_selector(move || format!("quote-row-{index}"))
        .flex()
        .flex_col()
        .min_w_0()
        .border_l(px(theme::QUOTE_RULE_W))
        .border_color(theme::paint::LINE)
        .pl(px(theme::SPACE_2))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_BODY)
        .child(
            div()
                .text_color(rgb(theme::INK_SUBTLE))
                .child(SharedString::from("Quote")),
        )
        .child(
            div()
                .text_color(rgb(theme::INK_STRONG))
                .line_clamp(theme::QUOTE_ROW_LINES)
                .text_ellipsis()
                .child(SharedString::from(reply.quote)),
        )
        .when(!reply.text.is_empty(), |row| {
            row.child(
                div()
                    .debug_selector(move || format!("quote-row-{index}-words"))
                    .text_color(rgb(theme::INK_STRONG))
                    .line_clamp(theme::QUOTE_ROW_LINES)
                    .text_ellipsis()
                    .child(SharedString::from(reply.text)),
            )
        })
        .children(source.map(|source| quote_source(index, source)))
}

/// Where a sent quote was cited from, when Ferrite kept it: the Block's
/// Go to source, or that it is no longer in this transcript.
pub struct QuoteSource {
    pub missing: bool,
    pub go: std::rc::Rc<dyn Fn(&mut Window, &mut App)>,
}

fn quote_source(index: usize, source: QuoteSource) -> gpui::Div {
    use ferrite_ui::theme;
    use gpui::{div, rgb};
    let row = div().debug_selector(move || format!("quote-row-{index}-source"));
    if source.missing {
        return row
            .text_color(rgb(theme::INK_SUBTLE))
            .child("Source unavailable");
    }
    row.child(
        ferrite_ui::components::button(("quote-source", index))
            .debug_selector(move || format!("quote-row-{index}-go"))
            .p_0()
            .h(px(theme::LH_META))
            .text_color(rgb(theme::LINK_INK))
            .child("Go to source")
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                (source.go)(window, cx);
            }),
    )
}

/// A tile's `×` in its top corner (an image card's, a reply's): clear until
/// the pointer is over the tile (`group`) or the keyboard reaches it — a
/// tab stop either way, in the `PromptAttachment` context.
fn tile_remove(
    selector: String,
    id: (&'static str, usize),
    group: SharedString,
    label: String,
    tooltip: String,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> gpui::Div {
    use ferrite_ui::theme;
    gpui::div()
        .debug_selector(move || selector.clone())
        .absolute()
        .top(px(theme::THUMB_REMOVE_INSET))
        .right(px(theme::THUMB_REMOVE_INSET))
        .child(
            ferrite_ui::components::button(id)
                .opacity(0.)
                .group_hover(group, |style| style.opacity(1.))
                .focus_visible(|style| ferrite_ui::components::control_focus(style).opacity(1.))
                .tab_stop(true)
                .p_0()
                .size(px(theme::THUMB_REMOVE))
                .rounded_full()
                .bg(theme::paint::FLOAT)
                .border_1()
                .border_color(theme::paint::LINE)
                .key_context("PromptAttachment")
                .accessibility_label(label)
                .tooltip_builder(ferrite_ui::menu::tooltip(tooltip))
                .child(ferrite_ui::icons::icon(
                    ferrite_ui::icons::CLOSE,
                    theme::ATTACH_REMOVE_ICON,
                    theme::INK_STRONG,
                ))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    on_click(window, cx);
                }),
        )
}

/// The draft's review comments as one `.att` chip: the pencil, `N review
/// comments` in `LINK_INK`, and a quiet `×` that drops them all.
fn review_chip(comments: usize, on_drop: Option<DropReview>) -> gpui::Div {
    use ferrite_ui::theme;
    use gpui::{div, rgb, SharedString};
    let label = format!(
        "{comments} review comment{}",
        if comments == 1 { "" } else { "s" }
    );
    div()
        .debug_selector(|| "review-attachment".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::CH))
        .h(px(theme::ATTACH_CHIP_H))
        .px(px(theme::SPACE_2))
        .rounded(px(theme::R_CONTROL))
        .bg(theme::paint::RAISED)
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_BODY)
        .text_color(rgb(theme::LINK_INK))
        .whitespace_nowrap()
        .child(ferrite_ui::icons::icon(
            ferrite_ui::icons::PENCIL,
            theme::ATTACH_THUMB,
            theme::LINK_INK,
        ))
        .child(SharedString::from(label))
        .when_some(on_drop, |chip, on_drop| {
            chip.child(
                remove_button("drop-review-comments")
                    .debug_selector(|| "drop-review-comments".into())
                    .accessibility_label("Remove the review comments")
                    .tooltip_builder(ferrite_ui::menu::tooltip("Remove the review comments"))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        on_drop(window, cx);
                    }),
            )
        })
}

fn retry_button(id: impl Into<ElementId>) -> gpui::component::button::Button {
    use ferrite_ui::theme;
    ferrite_ui::components::button(id)
        .tab_stop(true)
        .px(px(theme::SPACE_1))
        .flex_shrink_0()
        .h(px(theme::ATTACH_CHIP_H))
        .rounded(px(theme::R_TIGHT))
        .key_context("PromptAttachment")
        .child(
            gpui::div()
                .text_size(px(theme::FS_META))
                .text_color(gpui::rgb(theme::LINK_INK))
                .child("Retry"),
        )
}

/// A file's state in words, in the place the file is: red when it blocks the
/// send, quiet when it only goes as a path.
fn status_label(id: impl Into<ElementId>, state: &Readiness) -> Option<gpui::Stateful<gpui::Div>> {
    use ferrite_ui::theme;
    let label = match state {
        Readiness::Uploading { sent, total } if *total > 0 => format!(
            "Uploading {}%",
            sent.saturating_mul(100)
                .checked_div(*total)
                .unwrap_or(0)
                .min(100)
        ),
        _ => state.label()?.to_string(),
    };
    let uploading = matches!(state, Readiness::Pending | Readiness::Uploading { .. });
    let ink = if matches!(state, Readiness::Failed(_)) {
        theme::RED
    } else {
        theme::INK_SUBTLE
    };
    Some(
        gpui::div()
            .id(id)
            .debug_selector(move || {
                if uploading {
                    "attachment-upload-progress"
                } else {
                    "attachment-status"
                }
                .into()
            })
            .flex_shrink_0()
            .text_color(gpui::rgb(ink))
            .child(SharedString::from(label)),
    )
}

/// The retry a failed file offers: its callback, when a look again could
/// change what it says. A file that is on the wrong machine stays so.
fn retry_of(state: Option<&Readiness>, retry: Option<Retry>) -> Option<Retry> {
    match state {
        Some(Readiness::Failed(
            Failure::Missing | Failure::Gone | Failure::Unreadable(_) | Failure::Upload(_),
        )) => {
            retry
        }
        _ => None,
    }
}

/// A chip's remove control: the `close` icon (`ATTACH_REMOVE_ICON`) in
/// `INK_SUBTLE` in an `ATTACH_REMOVE`-wide slot, the kit ghost's fill under the
/// pointer — a tab stop in the `PromptAttachment` context, so its Enter
/// never sends.
pub(crate) fn remove_button(id: impl Into<ElementId>) -> gpui::component::button::Button {
    use ferrite_ui::theme;
    ferrite_ui::components::button(id)
        .tab_stop(true)
        .p_0()
        .flex_shrink_0()
        .h(px(theme::ATTACH_CHIP_H))
        .w(px(theme::ATTACH_REMOVE))
        .rounded(px(theme::R_TIGHT))
        .key_context("PromptAttachment")
        .child(ferrite_ui::icons::icon(
            ferrite_ui::icons::CLOSE,
            theme::ATTACH_REMOVE_ICON,
            theme::INK_SUBTLE,
        ))
}

impl RenderOnce for Attachments {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // The draft's shelf and the delivered prompt's files are one recipe;
        // only the draft can remove, or grow a file in.
        chips(self)
    }
}

/// A file link in prose, as T3's `MarkdownFileLink`: a chip, the file's
/// icon and the path as the prose wrote it in medium weight and the mention
/// accent, a `:line` suffix in `INK_SUBTLE`, underlined only under the
/// pointer (`link_chip`). It is a path target (`file_links::wire_inline`):
/// ⌘-click opens the Thread's preview, and a plain click opens it in the
/// Pane's preview too, at its `:line`. The native Markdown flow reserves the
/// returned size and wraps the link atomically, so the width is measured in
/// the face and size it is drawn in (the prose's).
pub fn inline_file(
    file: crate::file_links::FileLink,
    label: &str,
    preview: Option<&Preview>,
    scope: Option<gpui::SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> (gpui::Size<gpui::Pixels>, gpui::AnyElement) {
    use ferrite_ui::theme;
    use gpui::component::button::ButtonVariants as _;
    use gpui::rgb;

    let base = file
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let location = file
        .location
        .as_ref()
        .map(|line| format!(":{line}"))
        .unwrap_or_default();
    // The path as the prose wrote it, its `:line` drawn apart.
    let label = label.trim();
    let label = label.strip_suffix(location.as_str()).unwrap_or(label);
    let name = if label.is_empty() {
        base.clone()
    } else {
        label.to_string()
    };
    let image = crate::image_viewer::is_image(&file.path);
    let size_px = window.text_style().font_size.to_pixels(window.rem_size());
    let chip = crate::link_chip::Chip {
        icon: if image {
            ferrite_ui::icons::IMAGE
        } else {
            ferrite_ui::icons::FILE
        },
        ink: theme::MENTION_INK,
        name: &name,
        location: &location,
    };
    let link_w = chip.width(size_px, window);
    let line_h = window.line_height();
    let size = gpui::size(link_w, line_h);
    let host = preview.cloned();
    let selector = format!("file-attachment-{}", file.path.display());
    let accessibility = format!("Open {name}");
    let mut target = crate::file_links::PathTarget::new(file.path.clone()).at_line(
        file.location
            .as_deref()
            .and_then(crate::file_links::location_line),
    );
    if let Some(scope) = scope {
        target = target.in_scope(scope);
    }
    let reader = target.clone();
    let open = move |event: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
        gpui::base::TextSelection::end(window, cx);
        cx.stop_propagation();
        if let Some(host) = host.as_ref().filter(|host| host.remote_files()) {
            host.open(file.path.clone(), file.line(), &[], window, cx);
            return;
        }
        if event.modifiers().secondary() {
            crate::file_links::open_target(&reader, window, cx);
            return;
        }
        match &host {
            Some(host) if file.path.exists() => {
                let line = file
                    .location
                    .as_deref()
                    .and_then(crate::file_links::location_line);
                host.open(file.path.clone(), line, &[], window, cx);
            }
            _ => file.open(window, cx),
        }
    };
    let link = chip.face(size_px, link_w, line_h).id("inline-file-chip");
    // The click and keyboard target lies over the link and draws nothing but
    // the focus ring.
    let clear: gpui::Hsla = gpui::transparent_black();
    let button = ferrite_ui::components::button("inline-file-action")
        .custom(ferrite_ui::pointer::button_variant(
            clear,
            rgb(theme::MENTION_INK).into(),
            clear,
            cx,
        ))
        .tab_stop(true)
        .key_context("PromptAttachment")
        .accessibility_label(accessibility)
        .absolute()
        .inset_0()
        .size_full()
        .min_w_0()
        .p_0()
        .rounded(px(theme::R_CONTROL))
        .on_click(open);
    let element = gpui::div()
        .id("inline-file")
        .debug_selector(move || selector.clone())
        .relative()
        .w(link_w)
        .h(size.height)
        .child(link)
        .child(button);
    (
        size,
        crate::file_links::wire_inline(element, target).into_any_element(),
    )
}
