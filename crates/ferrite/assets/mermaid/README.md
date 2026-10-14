# Mermaid

`mermaid.min.js` is `dist/mermaid.min.js` from the npm package
[`mermaid@12.2.0`](https://www.npmjs.com/package/mermaid/v/12.2.0), unchanged
(MIT, `LICENSE` is the package's own; the licenses of the libraries it bundles
are kept in the file's trailing comment). It draws ` ```mermaid ` fences as
diagrams (`src/visual/diagram.rs`, ADR 0014), inlined into each diagram's page:
nothing is fetched.

sha256 `a8a6a6f805b074a8261a2825c86f99c8054acb3b35f50bb7e24a4f9f2c3c3b9f`
(package tarball `071914164d0f6d53e5c26a10a220230d4ae8d37ed0f2263fbf072266f0af3a0a`).

To update: `npm pack mermaid@<version>`, copy `package/dist/mermaid.min.js`
and `package/LICENSE` here, update the version and hashes above, and run
`cargo test -p ferrite diagram` (the bundle must still expose the `mermaid`
global and hold no `<script` or `</script`, since it is inlined in a page).
