# Development Environment

## Baseline toolchain

- Rust stable matching `rust-toolchain.toml`;
- Python 3.13 or a compatible current version;
- Node.js 22 LTS;
- pnpm 10 via Corepack;
- Buf/protoc when protocol generation begins;
- Docker only for disposable integration dependencies.

## Bootstrap

```bash
corepack enable
pnpm install --no-frozen-lockfile
python tools/verify_repo.py
python tools/verify_docs.py
cargo test --workspace
```

The generated repository intentionally does not contain a fabricated lockfile created without resolving dependencies. The first implementation commit that installs JavaScript dependencies must generate and commit `pnpm-lock.yaml`; CI should then switch to `--frozen-lockfile`.

## Local development profiles

- `development`: in-process/fake dependencies where possible;
- `local-only`: Node also hosts Head and canonical embedded Memory;
- `personal-server`: separate Head and optionally `memoryd`;
- `integration`: disposable PostgreSQL/object store/fake external services.

## Verification before a change

1. Read root and nearest nested `AGENTS.md`.
2. Run repository and documentation checks.
3. Run the smallest relevant test suite.
4. For protocol/state changes, add compatibility and scenario coverage.
