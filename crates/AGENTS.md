
# Rust Core Instructions

Crates contain stable domain/application logic and ports. Keep them independent of Tauri, React Native, provider SDKs and physical persistence unless the crate is explicitly an adapter.

- Favor small public APIs and explicit constructors.
- Put invariants in domain types or pure functions.
- Keep effects in application shells/adapters.
- Use `dennett-contracts` only for genuinely cross-boundary types.
- Do not create one crate per class; create a crate only for a stable ownership boundary.
- Every port should have at least one fake and a conformance test plan.
