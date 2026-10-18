//! Windows only: embed the app icon and version info (VERSIONINFO) into `guitario.exe`.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `GUITARIO_REQUIRE_WINRES=1` (for release
//! builds) turns it into an error.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/guitario.ico");
    println!("cargo:rerun-if-env-changed=GUITARIO_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/guitario.ico")
        .set("ProductName", "Guitario")
        .set("FileDescription", "Guitario tablature and notation editor")
        .set("LegalCopyright", "Copyright (c) the Guitario authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "guitario.exe")
        .set("InternalName", "guitario");
    if let Err(e) = res.compile() {
        assert!(std::env::var_os("GUITARIO_REQUIRE_WINRES").is_none(), "embedding Windows resources failed: {e}");
        println!("cargo:warning=guitario.exe built without icon/version resources: {e}");
    }
}
