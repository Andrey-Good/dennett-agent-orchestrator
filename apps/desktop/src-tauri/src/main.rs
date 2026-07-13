
#[tauri::command]
async fn project_chat(text: String) -> Result<String, String> {
    // Repository skeleton only. Production code must call denet-node over typed local IPC.
    Ok(format!("Queued for denet-node: {text}"))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![project_chat])
        .run(tauri::generate_context!())
        .expect("error while running Denet desktop shell");
}
