use dennett_observability::{DiagnosticsError, LocalDiagnosticsConfig, init_local, inspect_local};

#[test]
fn subscriber_failure_rolls_back_the_active_lifecycle_marker() {
    let temp = tempfile::tempdir().expect("temporary diagnostics");
    tracing_subscriber::fmt()
        .with_test_writer()
        .try_init()
        .expect("reserve process subscriber");

    let error = init_local(LocalDiagnosticsConfig::personal_quiet(
        "dennett-startup-test",
        temp.path(),
    ))
    .err()
    .expect("subscriber conflict");
    assert!(matches!(error, DiagnosticsError::SubscriberInitialization));

    let summary = inspect_local(temp.path(), "dennett-startup-test").expect("diagnostic summary");
    assert!(summary.active_runs.is_empty());
    assert_eq!(summary.unreadable_active_runs, 0);
    assert_eq!(
        summary.previous_exit,
        dennett_observability::ExitStatus::Unknown
    );
}
