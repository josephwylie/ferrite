---
status: accepted
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
