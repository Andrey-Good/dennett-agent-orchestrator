from __future__ import annotations

from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from tools import bootstrap


ROOT = Path(__file__).resolve().parents[2]


class BootstrapContractTests(unittest.TestCase):
    def test_python_bootstrap_uses_uv_without_pip(self) -> None:
        surfaces = [
            ROOT / "Justfile",
            ROOT / "tools" / "bootstrap.py",
            ROOT / ".github" / "workflows" / "ci.yml",
        ]
        content = "\n".join(path.read_text(encoding="utf-8") for path in surfaces)

        self.assertIn("uv sync", content)
        self.assertNotIn("pip install", content)
        self.assertNotIn("python -m pip", content)

    def test_bootstrap_subprocesses_are_non_interactive(self) -> None:
        content = (ROOT / "tools" / "bootstrap.py").read_text(encoding="utf-8")
        self.assertIn('"CI": "true"', content)

    def test_bootstrap_does_not_require_provider_credentials(self) -> None:
        content = (ROOT / "tools" / "bootstrap.py").read_text(encoding="utf-8")
        for variable in (
            "ANTHROPIC_API_KEY",
            "AZURE_OPENAI_API_KEY",
            "GOOGLE_API_KEY",
            "OPENAI_API_KEY",
        ):
            self.assertNotIn(variable, content)

    def test_local_config_creation_preserves_existing_file(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / ".env.example").write_text("SAFE=default\n", encoding="utf-8")

            bootstrap.create_local_config(root)
            self.assertEqual((root / ".env").read_text(encoding="utf-8"), "SAFE=default\n")

            (root / ".env").write_text("SAFE=custom\n", encoding="utf-8")
            bootstrap.create_local_config(root)
            self.assertEqual((root / ".env").read_text(encoding="utf-8"), "SAFE=custom\n")


if __name__ == "__main__":
    unittest.main()
