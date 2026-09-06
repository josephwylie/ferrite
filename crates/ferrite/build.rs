fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/app-icon.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/app-icon.ico")
            .compile()
            .expect("embed the Ferrite application icon");
    }
}
