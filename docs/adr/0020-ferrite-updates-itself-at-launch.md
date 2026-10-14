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
- Nothing is installed while Ferrite runs. A downloaded release is offered
  (a toast, Settings, the palette's `restart to update Ferrite`): restart now
  if no agent is working or waiting, else restart as soon as none is. The
  "Restart now" that follows asks first, as ⌘Q does. Declined, the release
  installs at the next launch whenever that is.
- The install is the first thing a launch does after opening the log, before
  any Session exists. macOS: mount the DMG, `ditto` its app beside the
  running bundle, swap the two by rename, and `exec` the new binary. Windows:
  a detached PowerShell waits for Ferrite to exit, runs the setup
  `/SILENT`, and starts Ferrite again. The manifest is marked `attempted`
  first; a launch that finds the mark while still the older version knows the
  install failed and opens the installer for the operator instead of trying
  again — as does a macOS install that fails outright (an app folder Ferrite
  cannot write, say).
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
- An update installed by hand shows no What's New (there are no staged notes
  to read), and a second Ferrite launched while the first runs would install
  under it; macOS opening the running app instead of a second copy makes that
  rare.
- Linux and Intel macOS have no installer and do not update themselves.
