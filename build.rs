//! Sets the shared library's SONAME (Linux/ELF) or install name (macOS) so
//! distro packagers and side-by-side installs of different major ABI
//! versions resolve the right `.so`/`.dylib` at runtime. Applies regardless
//! of the `c-api` feature, since the `cdylib` crate-type always builds.

use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=src/c_api.rs");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let abi_major = abi_major_version();

    // `cargo:rustc-cdylib-link-arg` only affects the `cdylib` output.
    match target_os.as_str() {
        "linux" | "android" | "freebsd" | "openbsd" | "netbsd" | "dragonfly" | "solaris"
        | "illumos" => {
            println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libpdf_inspector.so.{abi_major}");
        }
        "macos" | "ios" | "tvos" | "watchos" => {
            println!(
                "cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libpdf_inspector.{abi_major}.dylib"
            );
        }
        // Windows and everything else have no SONAME-equivalent concept here.
        _ => {}
    }
}

/// Read `PDF_INSPECTOR_ABI_VERSION` out of `src/c_api.rs` so the SONAME can
/// never drift from the ABI version it tracks.
fn abi_major_version() -> u32 {
    const NEEDLE: &str = "pub const PDF_INSPECTOR_ABI_VERSION: u32 = ";

    let source = fs::read_to_string("src/c_api.rs").expect("read src/c_api.rs for ABI version");
    source
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix(NEEDLE)?;
            rest.trim_end_matches(';').trim().parse::<u32>().ok()
        })
        .expect("find `PDF_INSPECTOR_ABI_VERSION: u32 = N;` in src/c_api.rs")
}
