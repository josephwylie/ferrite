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
