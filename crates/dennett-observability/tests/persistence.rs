use dennett_observability::{
    DiagnosticEvent, DiagnosticExit, ExitStatus, LocalDiagnosticsConfig, MarkerState, init_local,
    inspect_local, record,
};
use std::{path::PathBuf, process::Command};
use uuid::Uuid;

const DATA_DIR_ENV: &str = "DENNETT_DIAGNOSTIC_TEST_DATA_DIR";
const MODE_ENV: &str = "DENNETT_DIAGNOSTIC_TEST_MODE";
const COMPONENT: &str = "dennett-test-service";

#[test]
fn diagnostics_survive_restart_and_reconcile_an_unfinished_process() {
    let temp = tempfile::tempdir().expect("temporary diagnostic directory");
    run_child(temp.path().to_path_buf(), "clean");
    let clean = inspect_local(temp.path(), COMPONENT).expect("clean summary");
    assert_eq!(clean.previous_exit, ExitStatus::Clean);
    assert_eq!(clean.active_runs.len(), 0);

    run_child(temp.path().to_path_buf(), "abandon");
    let abandoned = inspect_local(temp.path(), COMPONENT).expect("abandoned summary");
    assert_eq!(abandoned.active_runs.len(), 1);
    assert_eq!(abandoned.active_runs[0].marker_state, MarkerState::Stale);

    run_child(temp.path().to_path_buf(), "clean");
    let recovered = inspect_local(temp.path(), COMPONENT).expect("recovered summary");
    assert_eq!(recovered.previous_exit, ExitStatus::Clean);
    assert_eq!(recovered.active_runs.len(), 0);
    assert!(recovered.log_file_count >= 1);

    let logs = read_logs(temp.path());
    assert!(logs.contains("diagnostics.initialized"));
    assert!(logs.contains("diagnostics.process_exit"));
    assert!(logs.contains("\"status\":\"unclean\""));
    assert!(logs.contains("019f0000-0000-7000-8000-000000000001"));
    assert!(logs.contains("019f0000-0000-7000-8000-000000000002"));
    assert!(logs.contains("019f0000-0000-7000-8000-000000000003"));
    let active_run = abandoned.active_runs[0].run_id.as_str();
    assert!(logs.contains(active_run));
}

#[test]
#[ignore = "process helper invoked by diagnostics_survive_restart_and_reconcile_an_unfinished_process"]
fn diagnostic_child_process() {
    let data_dir = PathBuf::from(std::env::var_os(DATA_DIR_ENV).expect("child data directory"));
    let mode = std::env::var(MODE_ENV).expect("child mode");
    let diagnostics = init_local(LocalDiagnosticsConfig::personal_quiet(COMPONENT, data_dir))
        .expect("initialize child diagnostics");
    record(
        DiagnosticEvent::info(
            "diagnostics.test_checkpoint",
            "test",
            "diagnostic persistence checkpoint",
        )
        .project_id(Uuid::parse_str("019f0000-0000-7000-8000-000000000001").expect("project UUID"))
        .session_id(Uuid::parse_str("019f0000-0000-7000-8000-000000000002").expect("session UUID"))
        .command_id(Uuid::parse_str("019f0000-0000-7000-8000-000000000003").expect("command UUID")),
    );
    if mode == "abandon" {
        drop(diagnostics);
    } else {
        diagnostics
            .shutdown(DiagnosticExit::Clean)
            .expect("clean diagnostic shutdown");
    }
}

fn run_child(data_dir: PathBuf, mode: &str) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .args([
            "--exact",
            "diagnostic_child_process",
            "--ignored",
            "--nocapture",
        ])
        .env(DATA_DIR_ENV, data_dir)
        .env(MODE_ENV, mode)
        .status()
        .expect("spawn diagnostic child");
    assert!(status.success(), "diagnostic child failed in {mode} mode");
}

fn read_logs(data_dir: &std::path::Path) -> String {
    let mut output = String::new();
    for entry in std::fs::read_dir(data_dir.join("diagnostics/logs")).expect("log directory") {
        let path = entry.expect("log entry").path();
        if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            output.push_str(&std::fs::read_to_string(path).expect("diagnostic log"));
        }
    }
    output
}
