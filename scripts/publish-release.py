#!/usr/bin/env python3
"""Publish verified Chronexus Server packages to its GitHub repository.

usage: publish-release.py VERSION NOTES.md ASSET [ASSET...]
The annotated tag must already be on origin. Existing releases/assets are never
replaced. Draft downloads and public downloads must match the local SHA-256.
The desktop updater is never used for server packages.
"""
import hashlib
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from github_release import GitHubReleaseClient, authorization

ROOT = Path(__file__).resolve().parents[1]
REPO = "Acelith/Chronexus-Server"
ORIGIN = "https://github.com/" + REPO + ".git"
ASSET = re.compile(r"chronexus-server-\d+\.\d+\.\d+-(?:linux-(?:amd64|arm64)|windows-x64\.exe)")


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True, stderr=subprocess.PIPE).strip()


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    version, notes, *names = sys.argv[1:]
    if not re.fullmatch(r"\d+\.\d+\.\d+", version) or not names:
        raise ValueError("A version and at least one verified package are required")
    assets = [Path(name) for name in names]
    for asset in assets:
        if (not ASSET.fullmatch(asset.name) or f"-{version}-" not in asset.name
                or asset.is_symlink() or not asset.is_file() or not 0 < asset.stat().st_size < 2 * 1024**3):
            raise ValueError(f"Unexpected asset: {asset.name}")
    if len({asset.name for asset in assets}) != len(assets):
        raise ValueError("Duplicate asset names")
    if git("status", "--porcelain") or git("remote", "get-url", "origin") != ORIGIN:
        raise ValueError("Clean tree and exact GitHub origin required")
    tag = "v" + version
    commit = git("rev-parse", tag + "^{commit}")
    remote = {line.split()[1]: line.split()[0] for line in git("ls-remote", "origin", f"refs/tags/{tag}^{{}}").splitlines()}
    if remote != {f"refs/tags/{tag}^{{}}": commit}:
        raise ValueError("The annotated source tag must be pushed to GitHub first")
    client = GitHubReleaseClient(authorization(), REPO)
    repository = client.api(client.prefix)
    if repository.get("full_name") != REPO or repository.get("private") or not repository.get("permissions", {}).get("push"):
        raise ValueError("Public repository with write access required")
    if any(release["tag_name"] == tag for release in client.api(client.prefix + "/releases?per_page=100")):
        raise ValueError("Release already exists; it is never replaced")
    with tempfile.TemporaryDirectory() as directory:
        sums = Path(directory) / "SHA256SUMS.txt"
        sums.write_text("".join(f"{sha(asset)}  {asset.name}\n" for asset in assets), encoding="utf-8")
        assets.append(sums)
        release = client.api(client.prefix + "/releases", "POST", {
            "tag_name": tag, "target_commitish": commit, "name": "Chronexus Server " + version,
            "body": Path(notes).read_text(encoding="utf-8"), "draft": True, "prerelease": False,
        })
        uploaded = []
        for asset in assets:
            expected = {"size": asset.stat().st_size, "sha256": sha(asset)}
            remote_asset = client.upload(release["id"], asset)
            client.verify(client.asset_api_url(remote_asset["id"]), expected)
            uploaded.append((remote_asset, expected))
        client.api(f"{client.prefix}/releases/{release['id']}", "PATCH", {"draft": False, "prerelease": False})
        for asset, expected in uploaded:
            client.verify(asset["browser_download_url"], expected, anonymous=True)
        print("Published and verified:", release["html_url"])


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # HTTP errors can contain signed CDN URLs; never include them in diagnostics.
        print("GitHub publication failed:", str(error) if isinstance(error, ValueError) else type(error).__name__, file=sys.stderr)
        raise SystemExit(1)
