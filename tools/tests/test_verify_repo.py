from __future__ import annotations

import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from tools import verify_repo


class TrackedJsonValidationTests(unittest.TestCase):
    def test_ignored_dependency_json_is_not_part_of_repository_validation(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            tracked = root / "planning" / "tracked.json"
            tracked.parent.mkdir(parents=True)
            tracked.write_text(json.dumps({"valid": True}), encoding="utf-8")

            ignored = root / "node_modules" / "dependency" / "tsconfig.json"
            ignored.parent.mkdir(parents=True)
            ignored.write_text("{ // valid JSONC, invalid canonical JSON\n}", encoding="utf-8")

            self.assertEqual(
                verify_repo.invalid_tracked_json(root, ["planning/tracked.json"]),
                [],
            )

    def test_invalid_tracked_json_is_reported(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "planning" / "invalid.json"
            path.parent.mkdir(parents=True)
            path.write_text("{ invalid", encoding="utf-8")

            invalid = verify_repo.invalid_tracked_json(
                root, ["planning/invalid.json"]
            )

            self.assertEqual(len(invalid), 1)
            self.assertEqual(invalid[0][0], "planning/invalid.json")


if __name__ == "__main__":
    unittest.main()
