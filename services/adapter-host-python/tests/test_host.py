
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))
from denet_adapter_host.main import handle


class HostTests(unittest.TestCase):
    def test_describe(self) -> None:
        response = handle({"id": "1", "method": "describe"})
        self.assertEqual(response["result"]["name"], "python-adapter-host")

    def test_unknown_method(self) -> None:
        response = handle({"id": "2", "method": "missing"})
        self.assertEqual(response["error"]["code"], "unknown_method")


if __name__ == "__main__":
    unittest.main()
