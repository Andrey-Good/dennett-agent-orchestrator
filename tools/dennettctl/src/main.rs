use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run(arguments: Vec<String>) -> Result<String, String> {
    match arguments.first().map(String::as_str).unwrap_or("help") {
        "status" => Ok("Dennett skeleton: no running installation discovered".to_owned()),
        "doctor" => doctor(&arguments[1..]),
        "help" | "--help" | "-h" => Ok(help()),
        command => Err(format!("unknown dennettctl command: {command}\n{}", help())),
    }
}

fn doctor(arguments: &[String]) -> Result<String, String> {
    doctor_with_default_data_dir(
        arguments,
        std::env::var_os("DENNETT_DATA_DIR").map(PathBuf::from),
    )
}

fn doctor_with_default_data_dir(
    arguments: &[String],
    mut data_dir: Option<PathBuf>,
) -> Result<String, String> {
    let mut component = "dennett-node".to_owned();
    let mut json = false;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--data-dir" => {
                index += 1;
                data_dir = arguments.get(index).map(PathBuf::from);
                if data_dir.is_none() {
                    return Err("--data-dir requires a path".to_owned());
                }
            }
            "--component" => {
                index += 1;
                component = arguments
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "--component requires a name".to_owned())?;
            }
            "--json" => json = true,
            value => return Err(format!("unknown doctor option: {value}")),
        }
        index += 1;
    }
    let data_dir = data_dir.ok_or_else(|| {
        "doctor needs --data-dir <path> or the DENNETT_DATA_DIR environment variable".to_owned()
    })?;
    let summary = dennett_observability::inspect_local(data_dir, &component)
        .map_err(|error| format!("diagnostics unavailable ({})", error.diagnostic_code()))?;
    if json {
        serde_json::to_string_pretty(&summary)
            .map_err(|_| "diagnostic summary could not be encoded".to_owned())
    } else {
        Ok(summary.to_string())
    }
}

fn help() -> String {
    "dennettctl commands:\n  status\n  doctor --data-dir <path> [--component dennett-node] [--json]"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_requires_an_explicit_profile_location() {
        let error = doctor_with_default_data_dir(&[], None).expect_err("missing data directory");
        assert!(error.contains("--data-dir"));
    }

    #[test]
    fn unknown_options_fail_instead_of_being_ignored() {
        let error =
            run(vec!["doctor".to_owned(), "--raw".to_owned()]).expect_err("unknown doctor option");
        assert!(error.contains("unknown doctor option"));
    }

    #[test]
    fn doctor_renders_the_typed_summary_without_reading_internal_tables() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = dennett_observability::init_local(
            dennett_observability::LocalDiagnosticsConfig::personal_quiet(
                "dennett-node",
                temp.path(),
            ),
        )
        .expect("initialize diagnostics");
        let live_text = doctor_with_default_data_dir(&[], Some(temp.path().to_path_buf()))
            .expect("live text diagnostic summary");
        assert!(live_text.contains("Active runs: 1 (live 1, stale 0, unreadable 0)"));
        assert!(!live_text.contains(temp.path().to_string_lossy().as_ref()));
        diagnostics
            .shutdown(dennett_observability::DiagnosticExit::Clean)
            .expect("complete diagnostics");

        let text = doctor_with_default_data_dir(&[], Some(temp.path().to_path_buf()))
            .expect("text diagnostic summary");
        assert!(text.contains("Dennett diagnostics: dennett-node"));
        assert!(text.contains("Previous exit: clean"));
        assert!(text.contains("dropped record(s)"));
        assert!(!text.contains(temp.path().to_string_lossy().as_ref()));

        let json =
            doctor_with_default_data_dir(&["--json".to_owned()], Some(temp.path().to_path_buf()))
                .expect("JSON diagnostic summary");
        let value: serde_json::Value = serde_json::from_str(&json).expect("summary JSON");
        assert_eq!(value["previous_exit"], "clean");
    }
}
