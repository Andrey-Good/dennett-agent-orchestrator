fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "project_chat",
        "native_mica_available",
        "open_system_watch",
        "close_system_watch",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to build Dennett desktop permissions");
}
