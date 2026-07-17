from __future__ import annotations

from pathlib import Path
import unittest

from tools import doctor


ROOT = Path(__file__).resolve().parents[2]


class DoctorTests(unittest.TestCase):
    def test_repository_pins_are_exact(self) -> None:
        self.assertEqual(
            doctor.expected_versions(ROOT),
            {
                "buf": "1.71.0",
                "cargo": "1.97.0",
                "just": "1.56.0",
                "node": "22.23.1",
                "pnpm": "10.0.0",
                "protoc": "35.1",
                "python": "3.13.5",
                "rustc": "1.97.0",
                "uv": "0.9.26",
            },
        )

    def test_mismatch_is_observable(self) -> None:
        expected = doctor.expected_versions(ROOT)
        outputs = {
            tuple(probe.command): _matching_output(probe.name, probe.expected)
            for probe in doctor.probes(expected)
        }
        outputs[("node", "--version")] = "v99.0.0"

        results = doctor.collect_results(expected, lambda command: outputs[tuple(command)])

        node = next(result for result in results if result.name == "node")
        self.assertFalse(node.ok)
        self.assertEqual(node.actual, "99.0.0")
        self.assertTrue(all(result.ok for result in results if result.name != "node"))


def _matching_output(name: str, version: str) -> str:
    prefixes = {
        "cargo": "cargo ",
        "just": "just ",
        "node": "v",
        "protoc": "libprotoc ",
        "python": "Python ",
        "rustc": "rustc ",
        "uv": "uv ",
    }
    return f"{prefixes.get(name, '')}{version}"


if __name__ == "__main__":
    unittest.main()
