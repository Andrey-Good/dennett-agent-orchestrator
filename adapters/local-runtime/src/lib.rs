//! Local loopback HTTP runtime adapter for self-hosted model servers
//! (Ollama, LM Studio, and `llama.cpp`).
//!
//! Architectural boundaries (Contract M, Architecture 82 §§1–3):
//! - Depends strictly on `dennett-contracts` (plus runtime/HTTP crates when the adapter
//!   implementation is wired).
//! - ZERO SQL drivers, ZERO UI frameworks, ZERO `dennett-memory-core`, and ZERO deferred
//!   provider crates.

use dennett_contracts::RuntimeAuthProfile;

/// Canonical runtime adapter identifier for local HTTP model servers (`local-http-runtime`).
pub const LOCAL_HTTP_ADAPTER_ID: &str = "local-http-runtime";

/// Canonical authentication profile used by [`LOCAL_HTTP_ADAPTER_ID`] (`local-endpoint`).
pub const LOCAL_HTTP_AUTH_PROFILE: RuntimeAuthProfile = RuntimeAuthProfile::LocalEndpoint;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_runtime_constants_match_contract() {
        assert_eq!(LOCAL_HTTP_ADAPTER_ID, "local-http-runtime");
        assert_eq!(LOCAL_HTTP_AUTH_PROFILE.as_method_id(), "local-endpoint");
    }
}
