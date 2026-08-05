fn main() {
    // build.rs 在「宿主」上运行，所以 `#[cfg(target_os = ..)]` 指的是宿主，
    // 交叉编译时不能用来判断目标平台。目标平台只能从 cargo 注入的
    // CARGO_CFG_TARGET_OS 读取。
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // Android sidecar 没有窗口，也没有 WebView：tauri_build::build() 会去解析
    // tauri.conf.json 的窗口/权限配置并生成 context，在这里既不需要也会失败。
    if target_os != "android" {
        tauri_build::build();
    } else {
        println!("cargo:warning=android target：跳过 tauri_build::build()（sidecar 无窗口）");
    }

    // Windows: Embed Common Controls v6 manifest for test binaries
    //
    // When running `cargo test`, the generated test executables don't include
    // the standard Tauri application manifest. Without Common Controls v6,
    // `tauri::test` calls fail with STATUS_ENTRYPOINT_NOT_FOUND.
    //
    // This workaround:
    // 1. Embeds the manifest into test binaries via /MANIFEST:EMBED
    // 2. Uses /MANIFEST:NO for the main binary to avoid duplicate resources
    //    (Tauri already handles manifest embedding for the app binary)
    if target_os == "windows" {
        let manifest_path = std::path::PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"),
        )
        .join("common-controls.manifest");
        let manifest_arg = format!("/MANIFESTINPUT:{}", manifest_path.display());

        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg={}", manifest_arg);
        // Avoid duplicate manifest resources in binary builds.
        println!("cargo:rustc-link-arg-bins=/MANIFEST:NO");
        println!("cargo:rerun-if-changed={}", manifest_path.display());
    }
}
