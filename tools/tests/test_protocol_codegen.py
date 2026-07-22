from __future__ import annotations

from copy import deepcopy
import hashlib
from pathlib import Path
import re
import subprocess
from tempfile import TemporaryDirectory
import unittest

import yaml

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
        self.assertIn(
            "boxed=.dennett.control.v1.WorkspaceWatchFrame.frame.snapshot", config
        )
        self.assertIn(
            "boxed=.dennett.control.v1.GetWorkspaceResponse.outcome.snapshot", config
        )
        self.assertIn(
            "boxed=.dennett.control.v1.GetWorkspaceResponse.outcome.error", config
        )

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
        document = yaml.safe_load(workflow)
        action_refs = re.findall(r"uses: [^@\s]+@([0-9a-f]+)", workflow)

        self.assertEqual(len(action_refs), 3)
        self.assertTrue(all(len(reference) == 40 for reference in action_refs))
        self.assertEqual(document["permissions"], {"contents": "read"})
        buf_steps = [
            step
            for step in document["jobs"]["buf"]["steps"]
            if step.get("uses", "").startswith("bufbuild/buf-setup-action@")
        ]
        self.assertEqual(
            buf_steps,
            [
                {
                    "uses": "bufbuild/buf-setup-action@"
                    "a47c93e0b1648d5651a065437926377d060baa99",
                    "with": {
                        "version": "1.71.0",
                        "github_token": "${{ github.token }}",
                    },
                }
            ],
        )
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

    def test_accepted_protocol_epoch_matches_exact_descriptor_contract(self) -> None:
        descriptor = protocol_codegen.build_descriptor_set()

        self.assertEqual(protocol_codegen.descriptor_contract_differences(descriptor), [])

        broken = deepcopy(descriptor)
        watch_file = next(
            file
            for file in broken["file"]
            if file.get("name") == "dennett/sync/v1/watch.proto"
        )
        watch_cursor = next(
            message
            for message in watch_file["messageType"]
            if message.get("name") == "WatchCursor"
        )
        authority_epoch = next(
            field
            for field in watch_cursor["field"]
            if field.get("name") == "authority_epoch"
        )
        authority_epoch["number"] = 99

        self.assertTrue(
            any(
                "WatchCursor fields" in difference
                for difference in protocol_codegen.descriptor_contract_differences(broken)
            )
        )

    def test_m02_workspace_mutations_admit_before_receipts_complete(self) -> None:
        descriptor = protocol_codegen.build_descriptor_set()
        messages = {
            f".{file['package']}.{message['name']}": message
            for file in descriptor["file"]
            if file.get("package") == "dennett.control.v1"
            for message in file.get("messageType", [])
        }
        mutating_requests = (
            "RegisterProjectRequest",
            "RebindProjectWorkspaceRequest",
            "SetProjectTrustRequest",
            "ApplyFileChangesRequest",
            "RunWorkspaceCommandRequest",
            "CancelWorkspaceOperationRequest",
            "CreateCheckpointRequest",
            "RestoreCheckpointRequest",
            "SubmitReviewActionRequest",
        )
        for short_name in mutating_requests:
            message = messages[f".dennett.control.v1.{short_name}"]
            command = next(field for field in message["field"] if field["number"] == 1)
            self.assertEqual(command["name"], "command", short_name)
            self.assertEqual(
                command["typeName"],
                ".dennett.common.v1.CommandMetadata",
                short_name,
            )

        accepted = messages[".dennett.control.v1.WorkspaceOperationAccepted"]
        command = next(field for field in accepted["field"] if field["name"] == "command")
        self.assertEqual(command["typeName"], ".dennett.common.v1.CommandAccepted")

        receipt = messages[".dennett.control.v1.WorkspaceOperationReceipt"]
        terminal_fields = {
            field["name"]: field["typeName"]
            for field in receipt["field"]
            if field.get("oneofIndex") == 0
        }
        self.assertEqual(
            terminal_fields,
            {
                "success": ".dennett.control.v1.WorkspaceOperationSuccess",
                "failure": ".dennett.control.v1.WorkspaceFailure",
            },
        )

        workspace_accepted = messages[
            ".dennett.control.v1.WorkspaceOperationAccepted"
        ]
        self.assertEqual(
            {field["name"] for field in workspace_accepted["field"]},
            {"command", "allocated_refs"},
        )
        cancel_accepted = messages[
            ".dennett.control.v1.CancelWorkspaceOperationAccepted"
        ]
        self.assertEqual(
            {field["name"] for field in cancel_accepted["field"]},
            {"command", "target_workspace_operation_id"},
        )

    def test_m02_portable_metadata_is_an_explicit_non_authority_choice(self) -> None:
        descriptor = protocol_codegen.build_descriptor_set()
        files = [
            file
            for file in descriptor["file"]
            if file.get("package") == "dennett.control.v1"
        ]
        enums = {
            f".dennett.control.v1.{enum['name']}": enum
            for file in files
            for enum in file.get("enumType", [])
        }
        messages = {
            f".dennett.control.v1.{message['name']}": message
            for file in files
            for message in file.get("messageType", [])
        }

        metadata_actions = tuple(
            (value["name"], value["number"])
            for value in enums[".dennett.control.v1.PortableMetadataAction"]["value"]
        )
        self.assertEqual(
            metadata_actions,
            (
                ("PORTABLE_METADATA_ACTION_UNSPECIFIED", 0),
                ("PORTABLE_METADATA_ACTION_LEAVE_ABSENT", 1),
                ("PORTABLE_METADATA_ACTION_USE_EXISTING", 2),
                ("PORTABLE_METADATA_ACTION_CREATE_MINIMAL", 3),
                ("PORTABLE_METADATA_ACTION_FORK_WITH_NEW_IDENTITY", 4),
            ),
        )
        rebind_actions = tuple(
            (value["name"], value["number"])
            for value in enums[
                ".dennett.control.v1.RebindPortableMetadataAction"
            ]["value"]
        )
        self.assertEqual(
            rebind_actions,
            (
                ("REBIND_PORTABLE_METADATA_ACTION_UNSPECIFIED", 0),
                ("REBIND_PORTABLE_METADATA_ACTION_LEAVE_ABSENT", 1),
                ("REBIND_PORTABLE_METADATA_ACTION_USE_EXISTING", 2),
                ("REBIND_PORTABLE_METADATA_ACTION_CREATE_MINIMAL", 3),
            ),
        )
        portable = messages[".dennett.control.v1.PortableProjectMetadata"]
        self.assertIn(
            "minimal_structure_creation_available",
            {field["name"] for field in portable["field"]},
        )
        policy = messages[".dennett.control.v1.ProjectAccessPolicy"]
        self.assertEqual(
            {field["name"] for field in policy["field"]},
            {"project_id", "trust_state", "revision", "policy_ref", "updated_at"},
        )

        encoded = str(descriptor).lower()
        for forbidden in ("codex", "sqlite", "pathbuf", "git_status", "git_diff"):
            self.assertNotIn(forbidden, encoded)

        non_streaming = deepcopy(descriptor)
        system_file = next(
            file
            for file in non_streaming["file"]
            if file.get("name") == "dennett/control/v1/system.proto"
        )
        system_service = next(
            service
            for service in system_file["service"]
            if service.get("name") == "SystemService"
        )
        watch_method = next(
            method
            for method in system_service["method"]
            if method.get("name") == "Watch"
        )
        watch_method.pop("serverStreaming")

        self.assertTrue(
            any(
                "SystemService methods" in difference
                for difference in protocol_codegen.descriptor_contract_differences(
                    non_streaming
                )
            )
        )

    def test_m02_trust_and_legacy_registration_are_fail_closed(self) -> None:
        descriptor = protocol_codegen.build_descriptor_set()
        project_file = next(
            file
            for file in descriptor["file"]
            if file.get("name") == "dennett/control/v1/project.proto"
        )
        messages = {
            message["name"]: message for message in project_file["messageType"]
        }

        register_fields = {
            field["name"]: field for field in messages["RegisterProjectRequest"]["field"]
        }
        self.assertEqual(
            register_fields["trust_decision"]["typeName"],
            ".dennett.common.v1.StableRef",
        )
        trust_fields = {
            field["name"]: field for field in messages["SetProjectTrustRequest"]["field"]
        }
        self.assertEqual(
            trust_fields["trust_decision"]["typeName"],
            ".dennett.common.v1.StableRef",
        )
        self.assertFalse(
            trust_fields["expected_policy_revision"].get("proto3Optional", False)
        )
        rebind_fields = {
            field["name"]: field
            for field in messages["RebindProjectWorkspaceRequest"]["field"]
        }
        self.assertEqual(
            rebind_fields["portable_metadata_action"]["typeName"],
            ".dennett.control.v1.RebindPortableMetadataAction",
        )

        project_root = next(
            field
            for field in messages["Project"]["field"]
            if field["name"] == "root_uri"
        )
        create_root = next(
            field
            for field in messages["CreateProjectRequest"]["field"]
            if field["name"] == "root_uri"
        )
        self.assertTrue(project_root["options"]["deprecated"])
        self.assertTrue(create_root["options"]["deprecated"])
        project_service = next(
            service
            for service in project_file["service"]
            if service["name"] == "ProjectService"
        )
        legacy_method = next(
            method
            for method in project_service["method"]
            if method["name"] == "CreateProject"
        )
        self.assertTrue(legacy_method["options"]["deprecated"])

    def test_m02_initial_descriptor_approval_detects_any_initial_field_change(
        self,
    ) -> None:
        descriptor = protocol_codegen.build_descriptor_set()
        files = {
            file["name"]: file
            for file in descriptor["file"]
            if file.get("name") in protocol_codegen.M02_INITIAL_DESCRIPTOR_FILES
        }
        self.assertEqual(
            protocol_codegen.m02_initial_descriptor_sha256(files),
            protocol_codegen.APPROVED_M02_INITIAL_DESCRIPTOR_SHA256,
        )

        broken = deepcopy(descriptor)
        workspace_file = next(
            file
            for file in broken["file"]
            if file.get("name") == "dennett/control/v1/workspace.proto"
        )
        snapshot = next(
            message
            for message in workspace_file["messageType"]
            if message["name"] == "WorkspaceSnapshot"
        )
        observed_at = next(
            field for field in snapshot["field"] if field["name"] == "observed_at"
        )
        observed_at["number"] = 99
        self.assertTrue(
            any(
                difference.startswith("M02 initial descriptor approval hash")
                for difference in protocol_codegen.descriptor_contract_differences(broken)
            )
        )

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
