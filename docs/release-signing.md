# Release signing

Tagged releases build usable unsigned installers when no platform signing
identity is configured. If some—but not all—secrets for a platform are set, the
job fails rather than silently publishing an unexpectedly unsigned build.
Packaging alone cannot remove operating-system trust warnings: the identity
must chain to Apple or a Windows-trusted certificate authority.

## macOS

Join the Apple Developer Program, create a **Developer ID Application**
certificate, and export it with its private key as a password-protected `.p12`.
Create an App Store Connect API key with permission to use the notary service.

Configure these GitHub Actions secrets:

- `MACOS_CERTIFICATE`: base64 of the exported `.p12`
- `MACOS_CERTIFICATE_PASSWORD`: the `.p12` export password
- `KEYCHAIN_PASSWORD`: a random password used only for the temporary CI keychain
- `APPLE_API_KEY_ID`: the App Store Connect API key ID
- `APPLE_API_ISSUER_ID`: the App Store Connect issuer ID
- `APPLE_API_PRIVATE_KEY`: the complete contents of the API `.p8` file

With all six secrets present, the release job signs the `.app` with hardened
runtime and a secure timestamp, places it in a drag-to-Applications `.dmg`,
signs the disk image, submits it with `notarytool`, staples the ticket, and
verifies it with `stapler` and Gatekeeper. With none present, it gives the app
an ad-hoc signature and publishes an unsigned DMG; users authorize its first
launch with **Control-click → Open**.

## Windows

Obtain an Authenticode code-signing certificate from a Windows-trusted
certificate authority and export it with its private key as a
password-protected `.pfx`. An organization-validation or extended-validation
certificate gives users the strongest publisher identity; reputation-based
SmartScreen prompts can still occur while a new certificate establishes
reputation. Reuse the same certificate for subsequent releases and timestamp
every signature so that reputation can accumulate and old builds remain valid
after the certificate expires. Authenticode removes the deterministic
**Unknown publisher** warning; SmartScreen reputation is controlled by
Microsoft and cannot be guaranteed by the installer format alone.

Configure these GitHub Actions secrets:

- `WINDOWS_CERTIFICATE`: base64 of the exported `.pfx`
- `WINDOWS_CERTIFICATE_PASSWORD`: the `.pfx` export password

With both secrets present, the release job signs the application executable,
builds a per-user Inno Setup installer, signs the installer and embedded
uninstaller, timestamps every signature, and verifies the resulting
Authenticode signatures. With neither present, it builds the same installer
unsigned; users pass the first SmartScreen prompt with **More info → Run
anyway**.

## Creating a release

Update the workspace version, commit it, and push a matching `v*` tag. The
release workflow publishes:

- `ferrite-<tag>-aarch64-apple-darwin.dmg`
- `ferrite-<tag>-x86_64-pc-windows-msvc-setup.exe`

Do not commit certificates or private keys. The workflow writes them only to
the hosted runner's temporary directory and removes them in an `always()` step.
