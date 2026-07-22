use dennett_observability::{
    DiagnosticEvent, DiagnosticExit, DiagnosticProvider, LocalDiagnosticsConfig, init_local, record,
};
use uuid::Uuid;

#[test]
fn persistent_logs_exclude_untyped_events_and_reject_unsafe_references() {
    const SECRET: &str = "sk-proj-private-token-0123456789";
    const GITHUB_SECRET: &str = "ghp_privateToken012345678901234567890";
    const JWT_SECRET: &str = "eyJhbGciOiJIUzI1NiJ9.private.signature";
    let temp = tempfile::tempdir().expect("temporary diagnostic directory");
    let diagnostics = init_local(LocalDiagnosticsConfig::personal_quiet(
        "dennett-redaction-test",
        temp.path(),
    ))
    .expect("initialize diagnostics");

    tracing::error!(
        prompt = SECRET,
        token = GITHUB_SECRET,
        credential = JWT_SECRET,
        "untyped private event"
    );
    tracing::error!(
        target: "dennett.private_safe_diagnostic",
        prompt = SECRET,
        "spoofed diagnostic target"
    );
    record(
        DiagnosticEvent::error(
            "runtime.provider_failure",
            "runtime",
            "provider operation failed",
        )
        .command_id(Uuid::now_v7())
        .provider(DiagnosticProvider::from_adapter_id(SECRET))
        .error_code("provider\nsecret")
        .retryable(true),
    );
    diagnostics
        .shutdown(DiagnosticExit::Failed {
            error_code: "provider_unavailable",
        })
        .expect("failed diagnostic shutdown");

    let logs = read_logs(temp.path());
    assert!(logs.contains("runtime.provider_failure"));
    assert!(logs.contains("provider_unavailable"));
    assert!(logs.contains("[invalid]"));
    assert!(!logs.contains(SECRET));
    assert!(!logs.contains(GITHUB_SECRET));
    assert!(!logs.contains(JWT_SECRET));
    assert!(logs.contains("\"provider_id\":\"other\""));
    assert!(!logs.contains("\"prompt\""));
    assert!(!logs.contains("\"token\""));
    assert!(!logs.contains("untyped private event"));
    assert!(!logs.contains("spoofed diagnostic target"));
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
