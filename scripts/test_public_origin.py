import hashlib
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("public_origin", Path(__file__).with_name("check-public-origin.py"))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class PublicOriginTests(unittest.TestCase):
    def test_raw_case_insensitive_and_encoded_hosts_are_detected(self):
        digest = hashlib.sha256(b"retired.example").hexdigest()
        for value in [b"https://retired.example/repo", b"HTTPS://RETIRED.EXAMPLE/repo",
                      b"https%3A%2F%2Fretired%2Eexample/repo", b"retired%252Eexample"]:
            self.assertTrue(audit.retired(value, digest))
        self.assertFalse(audit.retired(b"https://retired.example.evil/repo", digest))
        self.assertFalse(audit.retired(b"https://github.com/Acelith/Chronexus", digest))


if __name__ == "__main__":
    unittest.main()
