use std::{env, path::Path};

fn main() {
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/vad/utf8-paths.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // Merge into the executable's manifest. The pinned C++ runtime opens UTF-8 narrow paths.
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
