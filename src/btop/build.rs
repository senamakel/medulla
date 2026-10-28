//! Compiles the vendored btop (`vendor/btop`) into a static library.
//!
//! btop is C++23 and has collectors only for Linux, macOS and the BSDs. On any
//! other target nothing is compiled and the crate is built with
//! `cfg(medulla_btop_unsupported)`, under which [`Btop::start`] reports
//! `Unsupported` instead of linking.
//!
//! [`Btop::start`]: ../src/lib.rs

use std::path::{Path, PathBuf};
use std::process::Command;

/// Sources shared by every platform, relative to `vendor/btop/src`.
const SHARED: [&str; 10] = [
    "btop.cpp",
    "btop_config.cpp",
    "btop_draw.cpp",
    "btop_embed.cpp",
    "btop_input.cpp",
    "btop_log.cpp",
    "btop_menu.cpp",
    "btop_shared.cpp",
    "btop_theme.cpp",
    "btop_tools.cpp",
];

fn main() {
    println!("cargo::rustc-check-cfg=cfg(medulla_btop_unsupported)");
    println!("cargo:rerun-if-env-changed=CXX");

    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"))
        .join("../../vendor/btop");
    let src = root.join("src");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let platform: &[&str] = match target_os.as_str() {
        "linux" => &["linux/btop_collect.cpp"],
        "macos" => &["osx/btop_collect.cpp", "osx/sensors.cpp", "osx/smc.cpp"],
        _ => {
            println!("cargo:rustc-cfg=medulla_btop_unsupported");
            return;
        }
    };

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++23")
        .include(root.join("include"))
        .include(&src)
        .define("FMT_HEADER_ONLY", None)
        .define("_FILE_OFFSET_BITS", "64")
        // Upstream's own build is -Wall -Wextra; its warnings are not ours to fix.
        .warnings(false)
        .flag_if_supported("-Wno-deprecated-declarations");
    if target_os == "linux" && std::env::var_os("CXX").is_none() {
        if let Some(compiler) = modern_gxx() {
            build.compiler(compiler);
        }
    }
    for file in SHARED.iter().chain(platform) {
        let path = src.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        build.file(path);
    }
    for header in watched_headers(&src).into_iter().chain([root.join("include/medulla_btop.h")]) {
        println!("cargo:rerun-if-changed={}", header.display());
    }
    build.compile("medulla_btop");

    if target_os == "macos" {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=IOKit");
    }
}

/// btop needs `std::ranges::to` and `std::expected`, which libstdc++ ships from
/// GCC 14. Ubuntu 24.04's default `g++` is 13 with `g++-14` installed beside
/// it, so prefer a versioned compiler when the default is too old.
fn modern_gxx() -> Option<&'static str> {
    let major = |cmd: &str| -> Option<u32> {
        let out = Command::new(cmd).arg("-dumpversion").output().ok()?;
        String::from_utf8(out.stdout).ok()?.trim().split('.').next()?.parse().ok()
    };
    if major("g++").is_some_and(|v| v >= 14) {
        return None;
    }
    ["g++-15", "g++-14"].into_iter().find(|cmd| major(cmd).is_some())
}

/// Headers whose edits must rebuild the library.
fn watched_headers(src: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(src)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "hpp"))
        .collect()
}
