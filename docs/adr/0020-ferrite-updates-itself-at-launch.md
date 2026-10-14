# 0020 — Ferrite updates itself at launch

Status: accepted 2026-10-15 (Herdr benchmark, Hygiene).

## Context

Ferrite reached its operators only through a release page they had to
revisit. The provider CLIs already update from inside the app
(`cli_updates`), but Ferrite itself is different in one way that decides
the design: every agent is a child of the app, so replacing the app while
it runs would end every Session mid-turn. The releases are built by
`release.yml` as one installer per platform (a DMG for macOS arm64, an Inno
Setup `.exe` for Windows x64) and may go out unsigned.

## Decision

- `ferrite_core::self_update` asks GitHub's `releases/latest` for
  `josephwylie/ferrite`, picks this platform's installer by the name
  `release.yml` gives it, and downloads it into `~/.ferrite/update` beside a
  `release.json` (version, notes, installer). The checksum is the SHA-256
  GitHub records for every asset on upload (`digest`); an asset without one
  is refused, and a download that does not match it is deleted. No separate
  checksum file is published: it would come from the same place over the
  same connection.
- The window checks 30 s after launch and every 6 hours while Settings ›
  About's "Check for updates automatically" is on (the default), or when the
  operator presses Check now. Only release builds running from an installed
  copy check (`ferrite_update::enabled`): a `cargo run` or a local
  `install-app.sh` build is a dev build and must not take the installed app's
  update.
- Only the Ferrite holding the store's claim updates. `main` opens the store
  before anything installs, and a second Ferrite (read-only, ADR 0009) neither
  installs, checks nor downloads. Otherwise a second Start-menu click with a
  release staged would run the setup, and Inno's `CloseApplications` closes
  whatever holds its files without asking in silent mode: the first Ferrite,
  agents and all. On macOS the same launch would swap the bundle under the
  running first copy. On `exec` the claim's file is closed, so the new
  Ferrite takes it.
- Nothing is installed while Ferrite runs. A downloaded release is offered
  (a toast, Settings, the palette's `restart to update Ferrite`): restart now
  if no agent is working or waiting, else restart as soon as none is. Work
  behind Main counts as working (`Thread::background_work`: Subagents and
  background tasks outlive Main's turn), so an unattended restart cuts
  nothing off. The "Restart now" that follows asks first, as ⌘Q does.
  Declined, the release installs at the next launch whenever that is.
- The install is the first thing a launch does after opening the log, before
  any Session exists. macOS: mount the DMG, `ditto` its app beside the
  running bundle, swap the two by rename, and `exec` the new binary. Windows:
  a detached PowerShell waits for Ferrite to exit, runs the setup
  `/SILENT`, and starts Ferrite again. The manifest is marked `attempted`
  first; a launch that finds the mark while still the older version knows the
  install failed and opens the installer for the operator instead of trying
  again — as does a macOS install that fails outright (an app folder Ferrite
  cannot write, say).
- A hand-over is recorded, not forgotten: the manifest is marked
  `handed_over` and kept with its installer. Later launches leave it alone,
  and a check that finds the same release again answers the staged one
  instead of downloading it, so Settings says it was handed over. Without
  the mark, every launch would re-download the installer and open it again.
  A newer release replaces it as usual; installing it by hand makes the next
  launch the staged version, which shows What's New. Settings' "Open
  installer" opens it again for an operator who closed it.
- The first launch of the installed version shows What's New: the release
  body, as Markdown, in the same reading dialog as the HTML preview. The
  staged directory is removed once read, so it shows once.

## Consequences

- Unsigned builds update without Gatekeeper or SmartScreen prompts: files
  Ferrite downloads itself carry no quarantine or Mark of the Web. The
  integrity check is the checksum over TLS; Ferrite does not check code
  signatures.
- A release with no installer for the platform, or without a checksum, is
  reported in Settings and never downloaded.
- A handed-over installer (a few hundred MB) stays in `~/.ferrite/update`
  until the operator installs it or a newer release replaces it.
- What's New shows the installed release's body only: the notes of releases
  skipped on the way (0.4 to 0.6 skips 0.5's) are not shown, and an update
  installed by hand without Ferrite having staged it shows none. No CHANGELOG
  is bundled; the release body is the one source of notes.
- A read-only second Ferrite still running when the claim holder's setup
  runs on Windows is closed by it. It runs no agents, so nothing is lost.
- macOS keys privacy grants (Full Disk Access, Automation, …) to the app's
  code signature. An unsigned or ad-hoc-signed release is a new identity, so
  those grants may need approving again after each update; a Developer ID
  signed release keeps them.
- Linux and Intel macOS have no installer and do not update themselves.
