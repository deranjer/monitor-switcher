fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/app_icon.ico")
            // ComCtl32 v6 - required for combo box dropdowns to auto-size
            // (see the comment in the manifest itself).
            .set_manifest_file("assets/app.manifest")
            .compile()
            .expect("failed to embed app icon resource");
    }
}
