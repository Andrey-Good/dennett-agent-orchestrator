use dennett_observability::{DiagnosticEvent, DiagnosticEventKind, init, record};
use std::{process::Command, time::Duration};

const COMPONENT: &str = "dennett-console-test";
const PRIVATE_TEXT: &str = "prompt-secret-must-not-reach-console";

#[test]
fn console_only_bootstrap_emits_registered_metadata_and_rejects_arbitrary_tracing() {
    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .args([
            "--exact",
            "console_only_child_process",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .expect("spawn console diagnostic child");
    assert!(output.status.success(), "console diagnostic child failed");

    let console = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(console.contains("diagnostics.console_initialized"));
    assert!(console.contains("diagnostics.test_checkpoint"));
    assert!(console.contains(COMPONENT));
    assert!(!console.contains(PRIVATE_TEXT));
}

#[test]
#[ignore = "process helper invoked by console_only_bootstrap_emits_registered_metadata_and_rejects_arbitrary_tracing"]
fn console_only_child_process() {
    init(COMPONENT);
    tracing::error!(target: "unregistered.private", private = PRIVATE_TEXT, "{PRIVATE_TEXT}");
    record(DiagnosticEvent::new(
        DiagnosticEventKind::DiagnosticsTestCheckpoint,
    ));
    std::thread::sleep(Duration::from_millis(50));
}
