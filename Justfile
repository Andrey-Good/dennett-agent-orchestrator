
set shell := ["bash", "-cu"]

default: verify

verify:
    python tools/verify_repo.py
    python tools/verify_docs.py
    python tools/verify_planning.py

rust:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

python:
    python -m unittest discover -s services/adapter-host-python/tests

ts:
    pnpm typecheck

all: verify rust python ts
