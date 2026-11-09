//! Inspect the OS token of an actual CEF renderer after it has produced pixels.
//! The script is embedded, so this exercises the installed application layout.

pub(super) fn assert_renderer_sandbox() -> Result<(), String> {
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            include_str!("windows-sandbox.ps1"),
        ])
        .env("FERRITE_SMOKE_BROWSER_PID", std::process::id().to_string())
        .output()
        .map_err(|e| format!("Windows renderer sandbox inspection: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "Windows renderer OS sandbox proof failed ({}): {stdout}{stderr}",
            output.status
        ));
    }
    print!("{stdout}");
    Ok(())
}
