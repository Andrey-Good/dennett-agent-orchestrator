# Structured Test Catalogue

Catalogue entries describe requirements to verify, not necessarily one test function each.

Read [`docs/testing/TEST_CATALOGUE_AND_QUALITY_GATES.md`](../../docs/testing/TEST_CATALOGUE_AND_QUALITY_GATES.md).

Rules:

- IDs are permanent.
- Critical entries must name requirement references and an owner.
- `expected` is changed only together with the owning specification/contract.
- `implementation.test_paths` points to actual automated tests after they exist.
- A manual test remains explicitly manual.
- Generated Markdown views must not become the source of truth.
