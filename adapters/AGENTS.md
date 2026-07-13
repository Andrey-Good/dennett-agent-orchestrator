
# Adapter Author Instructions

1. Read specification 41 and architecture volume 82.
2. Implement an existing port; do not add provider types to domain core.
3. Add a descriptor, capability probes, health semantics, cancellation/deadline behavior and usage reporting.
4. Preserve provider-native features through typed `native_extensions` rather than flattening them away.
5. Classify every tool/external effect and route it through Trust/Effect boundaries.
6. Treat output as untrusted data.
7. Add conformance tests, recorded fixtures, timeout/crash behavior and a deprecation path.
8. A new adapter must not require changing unrelated clients or canonical data.
