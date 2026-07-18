from __future__ import annotations

import hashlib
from pathlib import Path
import re
import subprocess
from tempfile import TemporaryDirectory
import unittest

from tools import protocol_codegen


ROOT = Path(__file__).resolve().parents[2]


class ProtocolCodegenTests(unittest.TestCase):
    def test_generation_plugins_are_version_and_revision_pinned(self) -> None:
        config = (ROOT / "protocols" / "buf.gen.yaml").read_text(encoding="utf-8")

        self.assertIn("buf.build/community/neoeinstein-prost:v0.5.0", config)
        self.assertIn("buf.build/community/neoeinstein-tonic:v0.5.0", config)
        self.assertIn("buf.build/bufbuild/es:v2.12.1", config)
        self.assertEqual(config.count("revision: 1"), 3)
        self.assertNotIn("buf.build/protocolbuffers/rust", config)

    def test_buf_configuration_matches_explicit_approval_hash(self) -> None:
        digest = hashlib.sha256(
            (ROOT / "protocols" / "buf.yaml").read_bytes()
        ).hexdigest()
        self.assertEqual(digest, protocol_codegen.APPROVED_BUF_CONFIG_SHA256)

    def test_buf_standard_configuration_has_no_lint_exceptions(self) -> None:
        config = (ROOT / "protocols" / "buf.yaml").read_text(encoding="utf-8")

        self.assertIn("use: [STANDARD]", config)
        self.assertNotIn("ignore", config)
        self.assertNotIn("except", config)

    def test_comparison_snapshot_uses_checker_owned_wire_json_config(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            destination = root / "destination"
            (source / "proto").mkdir(parents=True)
            (source / "proto" / "test.proto").write_text(
                'syntax = "proto3";\n',
                encoding="utf-8",
            )
            (source / "buf.yaml").write_text(
                "candidate-controlled: true\n",
                encoding="utf-8",
            )

            protocol_codegen.snapshot_protocol_module(
                source,
                destination,
                protocol_codegen.COMPARISON_BUF_CONFIG,
            )

            self.assertEqual(
                (destination / "buf.yaml").read_text(encoding="utf-8"),
                protocol_codegen.COMPARISON_BUF_CONFIG,
            )
            self.assertNotIn(
                "candidate-controlled",
                (destination / "buf.yaml").read_text(encoding="utf-8"),
            )

    def test_epoch_manifest_matches_explicit_approval_hash(self) -> None:
        digest = hashlib.sha256(
            protocol_codegen.EPOCH_MIGRATION_MANIFEST.read_bytes()
        ).hexdigest()

        self.assertEqual(digest, protocol_codegen.APPROVED_EPOCH_MIGRATION_SHA256)
        migration = protocol_codegen.load_epoch_migration()
        self.assertEqual(migration.owner_gate, "WP-M01-002")
        self.assertEqual(migration.retired_packages, ("dennett.v1",))

    def test_epoch_migration_requires_exact_tree_hashes_and_packages(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            baseline = root / "baseline"
            candidate = root / "candidate"
            self._write_proto(
                baseline,
                "dennett/v1/common.proto",
                "dennett.v1",
                "message Old {}",
            )
            self._write_proto(
                candidate,
                "dennett/common/v1/common.proto",
                "dennett.common.v1",
                "message New {}",
            )
            same_epoch = root / "same-epoch"
            self._write_proto(
                same_epoch,
                "dennett/v1/replacement.proto",
                "dennett.v1",
                "message Replacement {}",
            )
            self.assertTrue(
                protocol_codegen.protocol_epoch_changed(baseline, candidate)
            )
            self.assertFalse(
                protocol_codegen.protocol_epoch_changed(baseline, same_epoch)
            )
            migration = protocol_codegen.EpochMigration(
                migration_id="test",
                previous_epoch="old",
                current_epoch="new",
                base_module_sha256=protocol_codegen.protocol_module_sha256(baseline),
                candidate_module_sha256=protocol_codegen.protocol_module_sha256(
                    candidate
                ),
                retired_packages=("dennett.v1",),
                introduced_packages=("dennett.common.v1",),
                retired_symbol_families=("old",),
                introduced_symbol_families=("new",),
                decision_ref="docs/decisions/test.md",
                owner_gate="WP-M01-002",
            )

            self.assertEqual(
                protocol_codegen.epoch_migration_differences(
                    baseline, candidate, migration
                ),
                [],
            )

            path = candidate / "proto" / "dennett" / "common" / "v1" / "common.proto"
            path.write_text(
                path.read_text(encoding="utf-8") + "\nmessage Extra {}\n",
                encoding="utf-8",
            )
            self.assertTrue(
                any(
                    difference.startswith("candidate module hash is")
                    for difference in protocol_codegen.epoch_migration_differences(
                        baseline, candidate, migration
                    )
                )
            )

    def test_json_lint_output_is_normalised_across_platforms(self) -> None:
        result = subprocess.CompletedProcess(
            args=["buf", "lint"],
            returncode=1,
            stdout=(
                b'{"path":"C:\\\\tmp\\\\proto\\\\sample.proto",'
                b'"type":"SERVICE_SUFFIX","message":"bad"}\n'
            ),
            stderr=b"",
        )

        self.assertEqual(
            protocol_codegen.parse_lint_violations(result),
            frozenset({("proto/sample.proto", "SERVICE_SUFFIX", "bad")}),
        )

    def test_protocol_workflow_pins_actions_and_event_base_commit(self) -> None:
        workflow = (
            ROOT / ".github" / "workflows" / "protocol-compatibility.yml"
        ).read_text(encoding="utf-8")
        action_refs = re.findall(r"uses: [^@\s]+@([0-9a-f]+)", workflow)

        self.assertEqual(len(action_refs), 3)
        self.assertTrue(all(len(reference) == 40 for reference in action_refs))
        self.assertIn("github_token: ${{ github.token }}", workflow)
        self.assertIn("if: github.event_name == 'pull_request'", workflow)
        self.assertIn('--base-ref "${{ github.event.pull_request.base.sha }}"', workflow)
        self.assertIn("if: github.event_name == 'push'", workflow)
        self.assertIn('--base-ref "${{ github.event.before }}"', workflow)

    def test_generated_artifacts_have_do_not_edit_header(self) -> None:
        files = [
            path
            for language in protocol_codegen.GENERATED_LANGUAGES
            for path in sorted((ROOT / "generated" / language).rglob("*"))
            if path.is_file()
        ]

        self.assertGreater(len(files), 0)
        for path in files:
            self.assertTrue(
                path.read_bytes().startswith(protocol_codegen.DO_NOT_EDIT_HEADER),
                path.relative_to(ROOT).as_posix(),
            )

    def test_m01_protocol_epoch_exposes_only_the_bounded_typed_surface(self) -> None:
        sources = {
            path.relative_to(protocol_codegen.PROTOCOLS).as_posix(): path.read_text(
                encoding="utf-8"
            )
            for path in protocol_codegen.proto_files()
        }

        self.assertEqual(
            set(sources),
            {
                "proto/dennett/common/v1/common.proto",
                "proto/dennett/control/v1/project.proto",
                "proto/dennett/control/v1/session.proto",
                "proto/dennett/control/v1/system.proto",
                "proto/dennett/sync/v1/watch.proto",
            },
        )
        combined = "\n".join(sources.values())
        self.assertNotIn("package dennett.v1;", combined)
        self.assertNotIn("google.protobuf.Any", combined)
        self.assertNotRegex(combined, r"bytes\s+payload\s*=")
        self.assertNotIn("MemoryService", combined)
        self.assertNotIn("VoiceService", combined)

        system = sources["proto/dennett/control/v1/system.proto"]
        for method in ("Handshake", "Bootstrap", "GetHealth"):
            self.assertRegex(system, rf"rpc {method}\({method}Request\)")

        project = sources["proto/dennett/control/v1/project.proto"]
        for method in ("CreateProject", "ListProjects", "GetProject"):
            self.assertRegex(project, rf"rpc {method}\({method}Request\)")

        session = sources["proto/dennett/control/v1/session.proto"]
        for method in ("CreateSession", "SendTurn", "CancelTurn", "WatchSession"):
            self.assertRegex(session, rf"rpc {method}\({method}Request\)")
        for contract in (
            "message SessionSnapshot",
            "message SessionDelta",
            "uint64 base_revision",
            "uint64 new_revision",
            "oneof frame",
            "ResyncRequired resync_required",
            "ErrorEnvelope error",
        ):
            self.assertIn(contract, session)

        common = sources["proto/dennett/common/v1/common.proto"]
        for field in (
            "string command_id",
            "string idempotency_key",
            "string correlation_id",
            "string message_key",
            "bool retryable",
            "bool user_action_required",
            "optional uint64 current_revision",
        ):
            self.assertIn(field, common)

    def test_header_application_is_idempotent(self) -> None:
        with TemporaryDirectory() as directory:
            generated = Path(directory)
            path = generated / "client.ts"
            path.write_bytes(b"// generated\n\n")

            self.assertEqual(protocol_codegen.add_do_not_edit_headers(generated), [path])
            first = path.read_bytes()
            self.assertEqual(
                first,
                protocol_codegen.DO_NOT_EDIT_HEADER + b"// generated\n",
            )
            self.assertEqual(protocol_codegen.add_do_not_edit_headers(generated), [])
            self.assertEqual(path.read_bytes(), first)

    def test_tree_difference_reports_exact_artifact_paths(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            actual = root / "actual"
            expected = root / "expected"
            actual.mkdir()
            expected.mkdir()
            (actual / "changed.ts").write_text("old", encoding="utf-8")
            (expected / "changed.ts").write_text("new", encoding="utf-8")
            (actual / "extra.ts").write_text("extra", encoding="utf-8")
            (expected / "missing.ts").write_text("missing", encoding="utf-8")

            self.assertEqual(
                protocol_codegen.tree_differences(actual, expected, "generated/ts"),
                [
                    "stale: generated/ts/changed.ts",
                    "unexpected: generated/ts/extra.ts",
                    "missing: generated/ts/missing.ts",
                ],
            )

    def test_pull_request_base_precedes_local_main(self) -> None:
        self.assertEqual(
            protocol_codegen.base_ref_candidates(
                None,
                {"GITHUB_BASE_REF": "release"},
            ),
            ["origin/release", "release"],
        )
        self.assertEqual(
            protocol_codegen.base_ref_candidates("known-commit", {}),
            ["known-commit"],
        )

    @staticmethod
    def _write_proto(
        module: Path,
        relative_path: str,
        package: str,
        declaration: str,
    ) -> None:
        path = module / "proto" / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            f'syntax = "proto3";\npackage {package};\n{declaration}\n',
            encoding="utf-8",
        )


if __name__ == "__main__":
    unittest.main()
