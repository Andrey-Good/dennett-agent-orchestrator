use dennett_observability::{
    DiagnosticEvent, DiagnosticExit, LocalDiagnosticsConfig, init_local, inspect_local, record,
};

#[test]
fn shutdown_flushes_the_queue_before_persisting_the_exact_drop_count() {
    let temp = tempfile::tempdir().expect("temporary diagnostics");
    let mut config = LocalDiagnosticsConfig::personal_quiet("dennett-drop-test", temp.path());
    config.max_log_bytes = 1;
    let diagnostics = init_local(config).expect("initialize bounded diagnostics");

    record(DiagnosticEvent::info(
        "diagnostics.capacity_probe",
        "test",
        "bounded writer capacity probe",
    ));
    diagnostics
        .shutdown(DiagnosticExit::Clean)
        .expect("shutdown diagnostics");

    let summary = inspect_local(temp.path(), "dennett-drop-test").expect("diagnostic summary");
    assert_eq!(summary.dropped_log_records, 3);
    assert_eq!(
        summary.previous_exit,
        dennett_observability::ExitStatus::Clean
    );
}
