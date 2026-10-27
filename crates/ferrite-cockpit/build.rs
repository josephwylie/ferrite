fn main() {
    // The titlebar's DEV badge is absent only from a release-pipeline
    // build, which sets FERRITE_RELEASE (`titlebar::DEV`). Cargo does not
    // track an `option_env!` on its own, so say so — otherwise a cached
    // build would keep whichever answer it compiled first.
    println!("cargo:rerun-if-env-changed=FERRITE_RELEASE");
}
