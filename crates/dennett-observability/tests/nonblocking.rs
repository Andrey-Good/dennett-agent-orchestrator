use dennett_observability::{
    DiagnosticEvent, DiagnosticEventKind, DiagnosticExit, LocalDiagnosticsConfig, init_local,
    record,
};
use fs2::FileExt;
use std::{
    fs::OpenOptions,
    time::{Duration, Instant},
};

#[test]
fn event_publication_does_not_wait_for_lifecycle_storage() {
    let temp = tempfile::tempdir().expect("temporary diagnostics");
    let diagnostics = init_local(LocalDiagnosticsConfig::personal_quiet(
        "dennett-nonblocking-test",
        temp.path(),
    ))
    .expect("initialize diagnostics");
    let maintenance = OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            temp.path()
                .join("diagnostics/lifecycle/dennett-nonblocking-test.maintenance.lock"),
        )
        .expect("maintenance lock");
    FileExt::lock_exclusive(&maintenance).expect("hold lifecycle storage");

    let started = Instant::now();
    for _ in 0..8 {
        record(DiagnosticEvent::new(
            DiagnosticEventKind::DiagnosticsCapacityProbe,
        ));
    }
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "event publication waited for diagnostic storage"
    );

    FileExt::unlock(&maintenance).expect("release lifecycle storage");
    diagnostics
        .shutdown(DiagnosticExit::Clean)
        .expect("shutdown diagnostics");
}
