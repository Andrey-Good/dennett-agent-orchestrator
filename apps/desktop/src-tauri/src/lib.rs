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

use std::sync::atomic::{AtomicBool, Ordering};

use commands::SharedBridge;
use tauri::Manager;
use tauri::window::{Color, Effect, EffectState, EffectsBuilder};

/// Shared runtime flag indicating whether native Windows 11 Mica backdrop succeeded on this OS.
#[derive(Debug)]
pub struct MicaAvailability(AtomicBool);

impl MicaAvailability {
    /// Creates a new [`MicaAvailability`] state handle.
    #[must_use]
    pub const fn new(available: bool) -> Self {
        Self(AtomicBool::new(available))
    }

    /// Returns whether native Mica backdrop is active on the main window.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Updates the detected Mica availability status.
    pub fn set_available(&self, available: bool) {
        self.0.store(available, Ordering::Relaxed);
    }
}

/// Launches the native Tauri 2 desktop application window.
///
/// # Panics
///
/// Panics if the default loopback bridge URL cannot be initialized or if the Tauri
/// application runtime fails to start.
#[allow(clippy::too_many_lines)]
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
    let mica_state = MicaAvailability::new(cfg!(target_os = "windows"));

    tauri::Builder::default()
        .manage(shared)
        .manage(mica_state)
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let mica_ok = if cfg!(target_os = "windows") {
                    let effects = EffectsBuilder::new()
                        .effect(Effect::Mica)
                        .state(EffectState::Active)
                        .build();
                    window.set_effects(Some(effects)).is_ok()
                } else {
                    false
                };

                if let Some(flag) = app.try_state::<MicaAvailability>() {
                    flag.set_available(mica_ok);
                }

                if !mica_ok {
                    // Solid dark fallback (#111215) on Windows 10 (< 22000) and Linux so a
                    // transparent window never renders as a black void or corrupt backdrop.
                    let _ = window.set_effects(None);
                    let _ = window.set_background_color(Some(Color(17, 18, 21, 255)));
                    let _ = window.eval(
                        "document.documentElement.classList.add('native-mica-unavailable'); \
                         document.documentElement.dataset.surface = 'solid-fallback';",
                    );
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::is_mica_available,
            commands::get_head_health,
            commands::set_head_url,
            commands::get_runtime_catalog,
            commands::pick_workspace_folder,
            commands::list_projects,
            commands::create_project,
            commands::update_project,
            commands::rebind_project,
            commands::detach_project_workspace,
            commands::archive_project,
            commands::unarchive_project,
            commands::remove_project,
            commands::delete_project,
            commands::list_project_files,
            commands::list_agents,
            commands::create_agent,
            commands::update_agent_config,
            commands::send_agent_message,
            commands::list_agent_messages,
            commands::list_sessions,
            commands::create_session,
            commands::update_session,
            commands::get_session_detail,
            commands::send_message,
            commands::cancel_run,
            commands::list_artifacts,
            commands::create_artifact,
            commands::get_artifact,
            commands::list_artifact_revisions,
            commands::apply_artifact_lifecycle,
            commands::get_state_snapshot,
            commands::get_events,
            commands::create_task,
            commands::list_tasks,
            commands::get_task_detail,
            commands::update_task,
            commands::send_task_signal,
            commands::reply_to_task,
            commands::cancel_task,
            commands::list_inbox_cards,
            commands::resolve_inbox_card,
            commands::list_permission_requests,
            commands::resolve_permission_request,
            commands::list_capability_grants,
            commands::revoke_capability_grant,
            commands::dispatch_effect,
            commands::list_effects,
            commands::reconcile_effect,
            commands::list_diagnostics,
            commands::flush_memory_outbox,
            commands::search_memory,
            commands::list_secrets,
            commands::set_secret,
            commands::delete_secret,
            commands::list_provider_connections,
            commands::create_provider_connection,
            commands::update_provider_connection,
            commands::delete_provider_connection,
            commands::probe_provider_connection,
            commands::activate_provider_connection,
            commands::list_capabilities,
            commands::create_capability,
            commands::discover_capabilities,
            commands::update_capability,
            commands::delete_capability,
            commands::rollback_capability,
            commands::set_project_capability_binding,
            commands::commit_memory_note,
            commands::get_memory_overview,
            commands::correct_memory_note,
            commands::get_memory_note_history,
            commands::list_memory_claims,
            commands::create_memory_claim,
            commands::supersede_memory_claim,
            commands::list_memory_conflicts,
            commands::resolve_memory_conflict,
            commands::list_memory_documents,
            commands::create_memory_document,
            commands::get_memory_document,
            commands::update_memory_document,
            commands::get_memory_document_versions,
            commands::forget_memory_item,
            commands::list_memory_tombstones,
            commands::export_memory_package,
            commands::import_memory_package,
            commands::trigger_memory_index_rebuild,
            commands::list_memory_consolidation_jobs,
            commands::create_memory_consolidation_job,
            commands::pause_memory_consolidation_job,
            commands::resume_memory_consolidation_job,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dennett desktop application");
}
