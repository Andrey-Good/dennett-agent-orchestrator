use dennett_observability::{
    DiagnosticEvent, DiagnosticEventKind, DiagnosticExit, LocalDiagnostics, LocalDiagnosticsConfig,
    init, init_local, record,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diagnostics = start_diagnostics();
    let config = match dennett_node::NodeConfig::from_environment() {
        Ok(config) => config,
        Err(error) => {
            record(
                DiagnosticEvent::new(DiagnosticEventKind::NodeConfigurationFailed)
                    .error_code(error.diagnostic_code()),
            );
            finish_diagnostics(
                diagnostics,
                DiagnosticExit::Failed {
                    error_code: error.diagnostic_code(),
                },
            );
            return Err(error.into());
        }
    };
    let result = dennett_node::run(config, async {
        if tokio::signal::ctrl_c().await.is_err() {
            record(
                DiagnosticEvent::new(DiagnosticEventKind::NodeShutdownSignalFailed)
                    .error_code("node.shutdown_signal_failure"),
            );
            std::future::pending::<()>().await;
        }
    })
    .await;
    match result {
        Ok(()) => {
            finish_diagnostics(diagnostics, DiagnosticExit::Clean);
            Ok(())
        }
        Err(error) => {
            record(
                DiagnosticEvent::new(DiagnosticEventKind::NodeRunFailed)
                    .error_code(error.diagnostic_code()),
            );
            finish_diagnostics(
                diagnostics,
                DiagnosticExit::Failed {
                    error_code: error.diagnostic_code(),
                },
            );
            Err(error.into())
        }
    }
}

fn start_diagnostics() -> Option<LocalDiagnostics> {
    let data_dir = dennett_node::diagnostic_data_dir_from_environment();
    match init_local(LocalDiagnosticsConfig::personal_quiet(
        "dennett-node",
        data_dir,
    )) {
        Ok(diagnostics) => Some(diagnostics),
        Err(error) => {
            init("dennett-node");
            eprintln!(
                "Dennett local diagnostics unavailable ({})",
                error.diagnostic_code()
            );
            None
        }
    }
}

fn finish_diagnostics(diagnostics: Option<LocalDiagnostics>, exit: DiagnosticExit) {
    if let Some(diagnostics) = diagnostics
        && let Err(error) = diagnostics.shutdown(exit)
    {
        eprintln!(
            "Dennett diagnostic shutdown incomplete ({})",
            error.diagnostic_code()
        );
    }
}
