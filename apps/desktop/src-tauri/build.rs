fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "native_mica_available",
        "open_system_watch",
        "close_system_watch",
        "open_project_chat",
        "close_project_chat",
        "create_chat",
        "send_project_turn",
        "cancel_project_turn",
        "save_composer_draft",
        "discard_composer_draft",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to build Dennett desktop permissions");
}
