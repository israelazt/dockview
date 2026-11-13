//! Puts the icon, the manifest (per-monitor DPI, no elevation) and the version
//! information into `dockview.exe`.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/dockview.manifest");
    println!("cargo:rerun-if-changed=build.rs");

    // The build script runs on the host; only a Windows target gets resources.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("assets/icon.ico")
        .set_manifest_file("assets/dockview.manifest")
        .set("ProductName", "Dockview")
        .set(
            "FileDescription",
            "Low-latency viewer for UVC capture cards",
        )
        .set("LegalCopyright", "MIT License");
    // Without the Windows SDK's resource compiler the program still builds,
    // only with no icon: a contributor should not be blocked by that.
    if let Err(e) = resource.compile() {
        println!("cargo:warning=could not embed the icon and manifest: {e}");
    }
}
