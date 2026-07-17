from __future__ import annotations

from pathlib import Path
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
