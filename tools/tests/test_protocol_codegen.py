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

    def test_structured_lint_debt_rejects_an_added_finding(self) -> None:
        added = (
            "proto/dennett/v1/control.proto",
            "SERVICE_SUFFIX",
            'Service name "NewLegacy" should be suffixed with "Service".',
        )
        actual = frozenset({*protocol_codegen.APPROVED_LINT_VIOLATIONS, added})

        self.assertEqual(
            protocol_codegen.lint_debt_differences(actual),
            [
                "new violation: proto/dennett/v1/control.proto [SERVICE_SUFFIX] "
                'Service name "NewLegacy" should be suffixed with "Service".'
            ],
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

    def test_header_application_is_idempotent(self) -> None:
        with TemporaryDirectory() as directory:
            generated = Path(directory)
            path = generated / "client.ts"
            path.write_bytes(b"// generated\n")

            self.assertEqual(protocol_codegen.add_do_not_edit_headers(generated), [path])
            first = path.read_bytes()
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


if __name__ == "__main__":
    unittest.main()
