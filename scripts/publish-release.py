#!/usr/bin/env python3
"""Publish a verified release of this repository to its fixed Forgejo origin.

No credential discovery: the already configured Git credential helper supplies an
authorization header in memory, only for this host and repository. The release is created as a
draft, assets are uploaded and re-downloaded anonymously after publication to verify their
checksums. Existing releases, tags and assets are never replaced.
usage: publish-release.py VERSION NOTES.md ASSET [ASSET...]
"""
import base64, hashlib, json, subprocess, sys, urllib.error, urllib.parse, urllib.request, uuid
from pathlib import Path

BASE = "https://github.com"
REPO = "acelith/Chronexus-Server"
API = f"/api/v1/repos/{REPO}"
ROOT = Path(__file__).resolve().parents[1]


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True, stderr=subprocess.PIPE).strip()


def authorization():
    result = subprocess.run(
        ["git", "-c", "credential.interactive=never", "credential", "fill"],
        input=f"url={BASE}/{REPO}.git\n\n", cwd=ROOT, text=True, capture_output=True,
        env={**__import__("os").environ, "GIT_TERMINAL_PROMPT": "0", "GCM_INTERACTIVE": "Never"},
    )
    fields = dict(l.split("=", 1) for l in result.stdout.splitlines() if "=" in l)
    if result.returncode or fields.get("host") != "github.com" or not fields.get("password"):
        raise SystemExit("Git credentials for github.com are unavailable.")
    return "Basic " + base64.b64encode(f"{fields['username']}:{fields['password']}".encode()).decode()


opener = urllib.request.build_opener(NoRedirect())


def call(auth, path, method="GET", body=None, raw=None, headers=None):
    if not (path == API or path.startswith(API + "/")):
        raise SystemExit("Unexpected API path.")
    request = urllib.request.Request(
        BASE + path, method=method, data=raw if raw is not None else (None if body is None else json.dumps(body).encode()),
        headers={"Authorization": auth, "Accept": "application/json", **({"Content-Type": "application/json"} if body is not None else {}), **(headers or {})},
    )
    with opener.open(request, timeout=300) as response:
        return json.loads(response.read(2_000_000) or b"null")


def sha(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    version, notes, *assets = sys.argv[1:]
    tag = f"v{version}"
    assets = [Path(a) for a in assets]
    if git("status", "--porcelain") or git("remote", "get-url", "origin") != f"{BASE}/{REPO}.git":
        raise SystemExit("Clean tree and exact origin required.")
    commit = git("rev-parse", f"{tag}^{{commit}}")
    remote = {l.split()[1]: l.split()[0] for l in git("ls-remote", "origin", f"refs/tags/{tag}^{{}}").splitlines()}
    if remote != {f"refs/tags/{tag}^{{}}": commit}:
        raise SystemExit("The annotated tag must be pushed to origin first.")
    auth = authorization()
    repo = call(auth, API)
    if repo.get("full_name") != REPO or repo.get("private") or not repo.get("permissions", {}).get("push"):
        raise SystemExit("Public owned repository with write access required.")
    existing = [r for r in call(auth, f"{API}/releases?limit=50") if r["tag_name"] == tag]
    if existing:
        raise SystemExit(f"Release {tag} already exists; it is never replaced.")
    release = call(auth, f"{API}/releases", "POST", {
        "tag_name": tag, "name": f"Chronexus Server {version}", "body": Path(notes).read_text(encoding="utf-8"),
        "draft": True, "prerelease": False,
    })
    for asset in assets:
        boundary = "chronexus-" + uuid.uuid4().hex
        data = (f'--{boundary}\r\nContent-Disposition: form-data; name="attachment"; filename="{asset.name}"\r\nContent-Type: application/octet-stream\r\n\r\n').encode() + asset.read_bytes() + f"\r\n--{boundary}--\r\n".encode()
        call(auth, f"{API}/releases/{release['id']}/assets?name={urllib.parse.quote(asset.name)}", "POST", raw=data,
             headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})
    final = call(auth, f"{API}/releases/{release['id']}", "PATCH", {"draft": False})
    print("Published:", final["html_url"])
    # Anonymous download check of every asset.
    for asset in assets:
        listed = next(a for a in final["assets"] if a["name"] == asset.name)
        url = listed["browser_download_url"]
        digest = hashlib.sha256()
        for _ in range(4):
            try:
                with urllib.request.build_opener(NoRedirect()).open(url, timeout=300) as response:
                    while block := response.read(1 << 20):
                        digest.update(block)
                break
            except urllib.error.HTTPError as error:
                if error.code in (301, 302, 303, 307, 308) and urllib.parse.urlsplit(urllib.parse.urljoin(url, error.headers["Location"])).netloc == "github.com":
                    url = urllib.parse.urljoin(url, error.headers["Location"])
                    continue
                raise
        if digest.hexdigest() != sha(asset):
            raise SystemExit(f"Checksum mismatch for {asset.name}.")
        print("verified anonymously:", asset.name, digest.hexdigest())


if __name__ == "__main__":
    try:
        main()
    except urllib.error.HTTPError as error:
        raise SystemExit(f"Forgejo HTTP {error.code}; nothing else was changed, no credentials logged.")
