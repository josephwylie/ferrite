---
status: accepted, amended by the 2026-10-24 update below
date: 2026-09-05
---

# Pane-owned prompt attachments and image previews

Use the GPUI Kit 0.6.0 stack established by ADR-0003. The Attachment family
renders both pending files and files in sent prompts. GroupBox contains the
compact Composer island; matching surfaces and concave shoulders join it to
the prompt. Native keyframes animate additions and honor reduced motion.

Restore the kit's stock surface tokens and rem inside attachment cards:
Ferrite's global theme otherwise hides their borders and shrinks them.
The rest of the app retains Ferrite's theme.

Each Pane owns one image-preview slot shared by its Composer and transcript.
The kit's Base Dialog owns keyboard focus and dismissal; its DialogContent,
Header, Title and Button components render inside the owning Pane's bounds.
The preview follows pane resizing and contains the image without distortion.
Activation stops propagation and replaces the slot, never stacking previews.

Composer stores file paths, not file bytes. One prompt-files module carries
references through the existing text-based queue, history and persistence.
Provider adapters translate those references: Codex localImage inputs and
Claude image content blocks for supported images, local paths for other
file types. Claude's native image reads are capped at 5 MiB each; larger or
unreadable images remain file references. The agent's tools and permissions
still determine how it reads other formats.

Provider parity extension: Claude also receives PDF attachments as native
base64 document blocks. Inline images and PDFs are limited to 5 MiB each and
20 MiB total per prompt; skipped attachments keep their file references. Codex
receives WAV, MP3, M4A, WebM and OGG attachments as native local audio input.
Other formats retain the existing reference/mention behavior. Both adapters use
the same attachment cards and persisted path representation.
## Transcript file links (2026-09-06)

Agent-authored local Markdown links use the same Attachment slots in a compact
inline row: filename, source location, file-type label, and icon or thumbnail.
The native text parser keeps the original Markdown; a GPUI Base link-renderer
extension preserves paragraph, list, table, wrapping, and selection behavior.
Each card has its own interaction identity and native button activation.

One file-link resolver handles absolute paths, file URLs, home-relative paths,
and paths relative to the Thread's checkout. It removes source-line annotations
and encodes a proper file URL before calling the OS opener. Raw paths previously
went unchanged to macOS's URL API, including source-line suffixes. Missing files
produce an in-app error message. Images open in the Pane's reader slot (see the
2026-10-24 amendment); other files (including sent-prompt attachments) open in
the system's associated application.
Web links retain the native link presentation and destination.

## Amendment (2026-10-24): images open in the reader slot

The modal image preview is gone. Images now open in the same reader slot
a text file opens in, beside the Pane (`attachment_preview::Body::Image`,
drawn by `image_viewer.rs`). The slot is laid out by the board tree, and
nothing dims. A sent image, a prompt's chip, a chip on the Composer's
shelf, a file link and a ⌘-clicked path all go through one entry point,
`Preview::open_image`, so they all behave the same way.

The image sits centred on the reader's ground. It is fitted inside the
slot's inset and never drawn larger than its own size, one image pixel
to one point. Its edge is a white-at-10% ring at `R_CHIP`. A click
toggles between fit and actual size around the pointer. ⌘= and ⌘- step
the zoom, and ⌘0 fits again; these ease interruptibly. A pinch zooms and
a drag or scroll pans, following the hand. When an image is opened among
others (a prompt's, the shelf's), ← and → step through them.

A second image swaps in place, crossfading. An image replaces a text
reader, as another file would. Esc closes the slot (never interrupting
the turn), as does ⌘W while the viewer holds focus, and the keyboard
returns to the Pane. The head keeps the original one click away, opened
by the OS at its canonical file URL. An image that cannot be read says
so quietly in the slot.

## Amendment (2026-10-24, later): previews open in the Panel

The reader slot beside the Pane is the Review's alone now (ADR 0021).
Every file preview — text at a line, rendered Markdown, a command's
output, an image — opens in the right Panel's Preview tab (SPEC §5.6.3),
which widens while it shows. The Pane still owns its preview
(`preview::Preview`), shared by its Composer, transcript and rendered
documents; `Preview::open` is the one way a file opens, image or not, so a
non-image chip, a sent file and a binary file open there too: a file the
preview cannot show says so, with its original a click away. The image
viewer is unchanged; its inset is the preview card's.


## Amendment (2026-11-15): readiness and retained bytes

A draft's files are still paths, but a path is no longer taken on faith.
`ferrite_core::attachment` inspects each one where it was added and again
when a send is attempted: `Ready`, `Reference` (an image or PDF over 5 MiB, or
an image format the providers do not take natively: it goes as a path) or
`Failed` (missing, unreadable, or a file of this computer bound for a
machine). A failed file stays in the draft and is marked in its own place in
the tray, with a retry when looking again could change the answer; Send is
refused while any file has failed, so the draft is never lost and a file
repaired at the same path sends without being re-added.

Ferrite does not upload a local file to a machine. A picker, drop or paste
bound for a machine is `Failed`; files picked from the Thread's own
workspace are the machine's paths and go as they are. `DraftContent.local`
records which files are of this computer, so the refusal survives a restart.
(An older draft has no `local`, and reads as workspace files.)

On send, each file is copied into `<store>/attachments/<hash>/<name>` and the
prompt names the copy, so history, retry and a restored draft keep the bytes
that went out. A pasted screenshot is kept there at paste time, not in a
per-process temp directory. A file over 64 MiB is not copied and stays a live
reference. Prompts sent before this change name live paths and keep doing
so; nothing is migrated.

Image previews no longer depend on the header reader that supplies pixel
sizes: a readable image is drawn, and only an unreadable file is a failed
tile. Facts are cached per file version (size and modified time), so a
file repaired at the same path recovers.
