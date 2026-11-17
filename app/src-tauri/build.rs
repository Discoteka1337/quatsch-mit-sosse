fn main() {
    // Embed Common Controls v6 in both the app and test harnesses through the linker.
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows_msvc {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    let locale_path = "../ui/src/locales/de.json";
    println!("cargo:rerun-if-changed={locale_path}");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    let locale: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(locale_path).expect("German locale is missing"),
    )
    .expect("German locale must be valid JSON");
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("tauri.conf.json").expect("Tauri configuration is missing"),
    )
    .expect("Tauri configuration must be valid JSON");
    assert_eq!(config["productName"], locale["appName"]);
    for window in config["app"]["windows"].as_array().unwrap() {
        assert_eq!(window["title"], locale["appName"]);
    }
    let mut attributes = tauri_build::Attributes::new();
    if windows_msvc {
        // Avoid a duplicate manifest in Tauri's binary-only resource file.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    tauri_build::try_build(attributes).expect("Tauri build setup failed");
}
