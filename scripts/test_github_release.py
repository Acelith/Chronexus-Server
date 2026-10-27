import hashlib
import io
import json
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest.mock import patch
from github_release import GitHubReleaseClient


class GitHubTransportTests(unittest.TestCase):
    def setUp(self):
        self.client = GitHubReleaseClient("Bearer test-only", "Acelith/Chronexus")
        self.public = "https://github.com/Acelith/Chronexus/releases/download/v1.0.0/latest.json"
        self.cdn = "https://release-assets.githubusercontent.com/github-production-release-asset/123/fixture?sig=test-only"
        self.expected = {"size": 4, "sha256": hashlib.sha256(b"test").hexdigest()}

    def test_only_the_selected_repository_and_official_cdn_can_be_requested(self):
        self.assertEqual(self.client.allowed_download(self.public), self.public)
        for url in [self.cdn, self.public + "?token=secret", self.public.replace("Acelith", "Other"),
                    self.public.replace("https:", "http:"), "https://api.github.com/user"]:
            with self.assertRaises(ValueError):
                self.client.allowed_download(url)
        for path in ["/user", "/repos/Other/Chronexus/releases", self.client.prefix + "/../user"]:
            with self.assertRaises(ValueError):
                self.client.api(path)

    def test_draft_asset_credentials_are_not_forwarded_to_the_signed_cdn(self):
        redirect = urllib.error.HTTPError("https://api.github.com", 302, "redirect", {"Location": self.cdn}, None)
        with patch.object(self.client.opener, "open", side_effect=[redirect, io.BytesIO(b"test")]) as opened:
            self.client.verify(self.client.asset_api_url(1), self.expected)
        self.assertIn("Authorization", opened.call_args_list[0].args[0].headers)
        self.assertNotIn("Authorization", opened.call_args_list[1].args[0].headers)

    def test_foreign_redirect_is_rejected_before_any_second_request(self):
        redirect = urllib.error.HTTPError("https://api.github.com", 302, "redirect", {"Location": "https://foreign.example/file"}, None)
        with patch.object(self.client.opener, "open", side_effect=redirect) as opened:
            with self.assertRaises(ValueError):
                self.client.verify(self.client.asset_api_url(1), self.expected)
            self.assertEqual(opened.call_count, 1)

    def test_public_downloads_are_anonymous_and_uploads_use_github_raw_assets(self):
        with patch.object(self.client.opener, "open", return_value=io.BytesIO(b"test")) as opened:
            self.client.verify(self.public, self.expected, anonymous=True)
            self.assertNotIn("Authorization", opened.call_args.args[0].headers)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "latest.json"
            path.write_bytes(b"test")
            with patch.object(self.client.opener, "open", return_value=io.BytesIO(json.dumps({"id": 1}).encode())) as opened:
                self.client.upload(10, path)
                request = opened.call_args.args[0]
                self.assertEqual(request.data, b"test")
                self.assertEqual(request.headers["Content-type"], "application/octet-stream")
                self.assertTrue(request.full_url.startswith("https://uploads.github.com/repos/Acelith/Chronexus/releases/10/assets?"))


if __name__ == "__main__":
    unittest.main()
