fn main() {
    // The titlebar's DEV badge is absent only from a release-pipeline
    // build, which sets FERRITE_RELEASE (`titlebar::DEV`). Cargo does not
    // track an `option_env!` on its own, so say so — otherwise a cached
    // build would keep whichever answer it compiled first.
    println!("cargo:rerun-if-env-changed=FERRITE_RELEASE");
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/app-icon.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/app-icon.ico")
            .compile()
            .expect("embed the Ferrite application icon");
    }
}
