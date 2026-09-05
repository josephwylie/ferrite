---
status: accepted
date: 2026-09-05
---

# Use GPUI Kit's matched renderer and components

The operator requested GPUI Kit's Attachment component for Composer files,
including image previews and overlay actions. Ferrite's GPUI 0.2.2 and
GPUI Component 0.5.1 pin predates that component. Upgrade the app to
GPUI Kit 0.6.0, which supplies its matched GPUI renderer, Component, and
assets. The `gpui` dependency name remains an alias for the kit facade.
The lockfile pins the renderer and transitive dependencies.

This deliberately supersedes ADR-0001's package pin, retaining native GPUI
and the headless core. Reuse Attachment's composition slots, scrolling,
buttons and Dialog. A shared attachment renderer serves Composer and sent
prompts, restoring the kit's stock surface tokens and rem within attachment
cards: Ferrite's global theme otherwise hides their borders and shrinks them.
The rest of the app retains Ferrite's theme. The panes24 spike remains on its
original standalone lockfile so its baseline is unchanged.

Composer stores file paths, not file bytes. One prompt-files module carries
references through the existing text-based queue, history and persistence.
Provider adapters translate those references: Codex localImage inputs and
Claude image content blocks for supported images, local paths for other
file types. Claude's native image reads are capped at 5 MiB each; larger or
unreadable images remain file references. The agent's tools and permissions
still determine how it reads other formats.
