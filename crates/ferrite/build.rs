fn main() {
    // Visuals on Windows (ADR 0013): libcef.dll is linked through its import
    // library, so without this an exe with no Chromium beside it (a lone
    // `cargo install` copy) would fail to start at all. Delay-loaded, it is
    // loaded on the first CEF call, which `visual::boot` makes only once it
    // has seen the DLL there. CEF's own build delay-loads it the same way.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if windows && std::env::var_os("CARGO_FEATURE_CEF").is_some() {
        println!("cargo:rustc-link-arg=/DELAYLOAD:libcef.dll");
    }
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/app-icon.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/app-icon.ico")
            .compile()
            .expect("embed the Ferrite application icon");
    }
}
