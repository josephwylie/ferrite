# ferrite-projects

Ferrite's coordinator ("Projects"): a coordinator conversation, parallel
worker Threads, shared memory and an overview of what needs the operator.

Vendored from [herdr-projects](https://github.com/eliasstravik/herdr-projects)
at commit `4e4548c3c43888e6be1c96906215998f07dc63f0` (version 0.2.34),
MIT licensed, Copyright (c) 2026 Elias Stravik — see `LICENSE`. Its `src/`,
`tests/`, `skill/`, `docs/`, `build.rs` and `scripts/link-command.sh` are
copied verbatim; the package is renamed `ferrite-projects` and keeps
upstream's edition.

Ferrite changes upstream files only at the seams, each marked `Ferrite:` in
a comment. `src/herdr.rs` is the adapter: upstream's types and signatures,
answered by Ferrite's Server API through the `ferrite` CLI. How the seams
are drawn and how upstream updates are merged is in
`docs/adr/0030-projects-vendored-from-herdr-projects.md`.
