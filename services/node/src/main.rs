use dennett_observability::{
    DiagnosticEvent, DiagnosticExit, LocalDiagnostics, LocalDiagnosticsConfig, init, init_local,
    record,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diagnostics = start_diagnostics();
    let config = match dennett_node::NodeConfig::from_environment() {
        Ok(config) => config,
        Err(error) => {
            record(
                DiagnosticEvent::error(
                    "node.configuration_failed",
                    "startup",
                    "Node configuration validation failed",
                )
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
                DiagnosticEvent::error(
                    "node.shutdown_signal_failed",
                    "shutdown",
                    "Node could not wait for the explicit shutdown signal",
                )
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
                DiagnosticEvent::error(
                    "node.run_failed",
                    "runtime",
                    "Node stopped after a handled runtime failure",
                )
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
