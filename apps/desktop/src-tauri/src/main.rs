#[tauri::command]
async fn project_chat(text: String) -> Result<String, String> {
    // Repository skeleton only. Production code must call dennett-node over typed local IPC.
    Ok(format!("Queued for dennett-node: {text}"))
}

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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            project_chat,
            native_mica_available
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
