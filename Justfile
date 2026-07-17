
set shell := ["sh", "-eu", "-c"]
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default: verify

bootstrap:
    mise install
    mise exec -- uv python install 3.13.5
    mise exec -- uv sync --project . --frozen
    mise exec -- uv run --project . --frozen python tools/bootstrap.py

doctor:
    mise exec -- uv run --project . --frozen python tools/doctor.py

verify:
    mise exec -- uv run --project . --frozen python tools/verify_repo.py
    mise exec -- uv run --project . --frozen python tools/verify_docs.py
    mise exec -- uv run --project . --frozen python tools/verify_planning.py
    mise exec -- uv run --project . --frozen python tools/generate_test_catalogue.py --check
    mise exec -- uv run --project . --frozen python tools/generate_doc_index.py --check
    mise exec -- uv run --project . --frozen python tools/generate_repository_metadata.py --check

generate:
    mise exec -- uv run --project . --frozen python tools/protocol_codegen.py generate

generate-test-catalogue:
    mise exec -- uv run --project . --frozen python tools/generate_test_catalogue.py

test-contracts:
    mise exec -- uv run --project . --frozen python tools/protocol_codegen.py check

rust:
    mise exec -- uv run --project . --frozen python tools/run_in_toolchain.py cargo fmt --check
    mise exec -- uv run --project . --frozen python tools/run_in_toolchain.py cargo clippy --workspace --all-targets -- -D warnings
    mise exec -- uv run --project . --frozen python tools/run_in_toolchain.py cargo test --workspace

python:
    mise exec -- uv run --project . --frozen python -m unittest discover -s services/adapter-host-python/tests
    mise exec -- uv run --project . --frozen python -m unittest discover -s tools/tests

ts:
    mise exec -- corepack pnpm typecheck

check: verify rust python ts test-contracts

all: check
