mod system_bridge;

use system_bridge::{
    CancelProjectTurnRequest, CloseProjectChatRequest, CloseSystemWatchRequest, CreateChatRequest,
    CreateChatResponse, DesktopBridge, DesktopProjectChatEvent, DesktopSystemEvent,
    DiscardComposerDraftRequest, OpenProjectChatRequest, OpenProjectChatResponse,
    OpenSystemWatchRequest, OpenSystemWatchResponse, SaveComposerDraftRequest,
    SaveComposerDraftResponse, SendProjectTurnRequest, SendProjectTurnResponse, UiSafeError,
};
use tauri::{Manager, WebviewWindow, ipc::Channel};

const WINDOWS_11_MICA_BUILD: u32 = 22_000;

fn mica_supported_for_build(build: Option<u32>) -> bool {
    build.is_some_and(|build| build >= WINDOWS_11_MICA_BUILD)
}

#[cfg(target_os = "windows")]
fn windows_build_number() -> Option<u32> {
    #[repr(C)]
    struct RtlOsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform_id: u32,
        service_pack: [u16; 128],
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(version: *mut RtlOsVersionInfo) -> i32;
    }

    let mut version = RtlOsVersionInfo {
        size: std::mem::size_of::<RtlOsVersionInfo>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform_id: 0,
        service_pack: [0; 128],
    };
    let status = unsafe { RtlGetVersion(&mut version) };
    (status >= 0).then_some(version.build)
}

#[cfg(not(target_os = "windows"))]
fn windows_build_number() -> Option<u32> {
    None
}

#[tauri::command]
fn native_mica_available() -> bool {
    mica_supported_for_build(windows_build_number())
}

#[tauri::command]
async fn open_system_watch(
    window: WebviewWindow,
    bridge: tauri::State<'_, DesktopBridge>,
    request: OpenSystemWatchRequest,
    on_event: Channel<DesktopSystemEvent>,
) -> Result<OpenSystemWatchResponse, UiSafeError> {
    bridge
        .open_system_watch(window.label().to_owned(), request, on_event)
        .await
}

#[tauri::command]
fn close_system_watch(
    window: WebviewWindow,
    bridge: tauri::State<'_, DesktopBridge>,
    request: CloseSystemWatchRequest,
) -> bool {
    bridge.close_system_watch(window.label(), &request)
}

#[tauri::command]
async fn open_project_chat(
    window: WebviewWindow,
    bridge: tauri::State<'_, DesktopBridge>,
    request: OpenProjectChatRequest,
    on_event: Channel<DesktopProjectChatEvent>,
) -> Result<OpenProjectChatResponse, UiSafeError> {
    bridge
        .open_project_chat(window.label().to_owned(), request, on_event)
        .await
}

#[tauri::command]
fn close_project_chat(
    window: WebviewWindow,
    bridge: tauri::State<'_, DesktopBridge>,
    request: CloseProjectChatRequest,
) -> bool {
    bridge.close_project_chat(window.label(), &request)
}

#[tauri::command]
async fn create_chat(
    bridge: tauri::State<'_, DesktopBridge>,
    request: CreateChatRequest,
) -> Result<CreateChatResponse, UiSafeError> {
    bridge.create_chat(request).await
}

#[tauri::command]
async fn send_project_turn(
    bridge: tauri::State<'_, DesktopBridge>,
    request: SendProjectTurnRequest,
) -> Result<SendProjectTurnResponse, UiSafeError> {
    bridge.send_project_turn(request).await
}

#[tauri::command]
async fn cancel_project_turn(
    bridge: tauri::State<'_, DesktopBridge>,
    request: CancelProjectTurnRequest,
) -> Result<(), UiSafeError> {
    bridge.cancel_project_turn(request).await
}

#[tauri::command]
async fn save_composer_draft(
    bridge: tauri::State<'_, DesktopBridge>,
    request: SaveComposerDraftRequest,
) -> Result<SaveComposerDraftResponse, UiSafeError> {
    bridge.save_composer_draft(request).await
}

#[tauri::command]
async fn discard_composer_draft(
    bridge: tauri::State<'_, DesktopBridge>,
    request: DiscardComposerDraftRequest,
) -> Result<bool, UiSafeError> {
    bridge.discard_composer_draft(request).await
}

fn main() {
    dennett_observability::init("dennett-desktop-shell");
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir().ok();
            app.manage(DesktopBridge::new(data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            native_mica_available,
            open_system_watch,
            close_system_watch,
            open_project_chat,
            close_project_chat,
            create_chat,
            send_project_turn,
            cancel_project_turn,
            save_composer_draft,
            discard_composer_draft
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dennett desktop shell");
}

#[cfg(test)]
mod tests {
    use super::{WINDOWS_11_MICA_BUILD, mica_supported_for_build};

    #[test]
    fn mica_requires_a_known_windows_11_build() {
        assert!(!mica_supported_for_build(None));
        assert!(!mica_supported_for_build(Some(WINDOWS_11_MICA_BUILD - 1)));
        assert!(mica_supported_for_build(Some(WINDOWS_11_MICA_BUILD)));
    }
}
