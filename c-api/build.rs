//! Give C shared-library consumers an unversioned runtime library name.
fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS")
        .unwrap_or_default()
        .as_str()
    {
        "linux" | "android" | "freebsd" | "openbsd" | "netbsd" | "dragonfly" | "solaris"
        | "illumos" => println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libpdf_inspector_c.so"),
        "macos" | "ios" | "tvos" | "watchos" => {
            println!(
                "cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libpdf_inspector_c.dylib"
            )
        }
        _ => {}
    }
}
