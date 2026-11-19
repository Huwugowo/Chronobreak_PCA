fn main() {
    if cfg!(feature = "history-command-benchmark")
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // Tauri supplies its manifest to binary targets only. Use the same
        // Common Controls dependency at link time so the test harness gets it too.
        let attributes = tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        tauri_build::try_build(attributes).expect("Tauri build setup");
        let manifest = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/history-command-benchmark.manifest"
        );
        println!("cargo:rerun-if-changed={manifest}");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{manifest}");
    } else {
        tauri_build::build();
    }
}
