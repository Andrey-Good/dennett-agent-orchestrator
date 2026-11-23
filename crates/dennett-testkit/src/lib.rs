//! Isolated testing infrastructure and D0 technical risk probes for Dennett.
//!
//! Provides:
//! - [`profile`]: isolated test profile descriptors and [`profile::FaultInjectionGuard`]
//! - [`redact`]: diagnostic secret redaction helpers
//! - [`pg_harness`]: PostgreSQL connectivity, transaction roundtrip, and cross-domain denial probes
//! - [`acp_probe`]: JSON-RPC 2.0 over stdio client for `agy acp` (ACP v1)

pub mod acp_probe;
pub mod pg_harness;
pub mod profile;
pub mod redact;
