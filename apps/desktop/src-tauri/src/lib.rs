//! Dennett Desktop Tauri 2 shell and thin client bridge (`dennett-desktop`).
//!
//! Strictly depends only on `dennett-contracts`, `dennett-head-client`, and Tauri 2 runtime
//! crates. Never links `dennett-head`, `dennett-memory-core`, `dennett-memoryd`, PostgreSQL
//! drivers, or any model-provider SDK (Architecture 83 §1).
//!
//! Closing the desktop window only exits the UI process and NEVER stops or signals
//! `dennett-head` or `dennett-memoryd` (V03).

pub mod bridge;
mod commands;

pub use bridge::{
    ALLOW_REMOTE_HEAD_ENV, BridgeError, DEFAULT_HEAD_URL, DesktopHealthSnapshot, HEAD_URL_ENV,
    HEALTH_PROBE_TIMEOUT, HeadBridge, HeadEndpointPolicy, validate_head_url,
};

use commands::SharedBridge;

/// Launches the native Tauri 2 desktop application window.
///
/// # Panics
///
/// Panics if the default loopback bridge URL cannot be initialized or if the Tauri
/// application runtime fails to start.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let bridge = match HeadBridge::from_env() {
        Ok(configured) => configured,
        Err(err) => {
            tracing::warn!(
                error = %err,
                fallback_url = DEFAULT_HEAD_URL,
                "invalid DENNETT_HEAD_URL; falling back to default loopback endpoint"
            );
            HeadBridge::new(DEFAULT_HEAD_URL, HeadEndpointPolicy::LoopbackOnly)
                .expect("default loopback Head URL is always valid")
        }
    };

    let shared = SharedBridge::new(bridge);

    tauri::Builder::default()
        .manage(shared)
        .invoke_handler(tauri::generate_handler![
            commands::get_head_health,
            commands::set_head_url,
            commands::list_projects,
            commands::create_project,
            commands::list_agents,
            commands::create_agent,
            commands::update_agent_config,
            commands::list_sessions,
            commands::create_session,
            commands::get_session_detail,
            commands::send_message,
            commands::cancel_run,
            commands::list_artifacts,
            commands::create_artifact,
            commands::get_artifact,
            commands::list_diagnostics,
            commands::flush_memory_outbox,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dennett desktop application");
}
