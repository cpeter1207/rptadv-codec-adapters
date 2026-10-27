//! Compile only the narrow FFmpeg struct-access glue; codec libraries stay dynamic.
use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=src/glue.c");
    println!("cargo:rerun-if-env-changed=CFLAGS");
    // cargo-llvm-cov supplies flags for the exact target, not the build host.
    let target_flags = format!("CFLAGS_{}", env::var("TARGET").unwrap().replace('-', "_"));
    println!("cargo:rerun-if-env-changed={target_flags}");
    let cflags = env::var(&target_flags)
        .or_else(|_| env::var("CFLAGS"))
        .unwrap_or_default();
    let directory = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let flags = Command::new("pkg-config")
        .args(["--cflags", "libavcodec", "libavutil"])
        .output()
        .expect("FFmpeg development metadata");
    assert!(
        flags.status.success(),
        "FFmpeg development metadata is missing"
    );
    let object = directory.join("glue.o");
    assert!(
        Command::new(env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .args([
                "-std=c11",
                "-fPIC",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-c",
                "src/glue.c",
                "-o"
            ])
            .arg(&object)
            .args(cflags.split_whitespace())
            .args(
                String::from_utf8(flags.stdout)
                    .expect("compiler flags")
                    .split_whitespace()
            )
            .status()
            .expect("C compiler")
            .success(),
        "FFmpeg glue compilation failed"
    );
    assert!(
        Command::new("ar")
            .arg("crs")
            .arg(directory.join("libcodec_ffmpeg_glue.a"))
            .arg(object)
            .status()
            .expect("archiver")
            .success(),
        "FFmpeg glue archive failed"
    );
    println!("cargo:rustc-link-search=native={}", directory.display());
    println!("cargo:rustc-link-lib=static=codec_ffmpeg_glue");
    println!("cargo:rustc-link-lib=avcodec");
    println!("cargo:rustc-link-lib=avutil");
}
