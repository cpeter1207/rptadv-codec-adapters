//! Shared build-only search path for separately staged external backend DSOs.
fn main() {
    println!("cargo:rerun-if-env-changed=RPTADV_CODEC_BACKEND_LIBDIR");
    if let Ok(directory) = std::env::var("RPTADV_CODEC_BACKEND_LIBDIR") {
        println!("cargo:rustc-link-search=native={directory}");
    }
}
