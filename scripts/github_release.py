"""Fixed-repository GitHub release transport; credentials never follow redirects."""
import hashlib
import json
import re
import subprocess
import urllib.error
import urllib.parse
import urllib.request

API_HOST = "https://api.github.com"
WEB_HOST = "https://github.com"
UPLOAD_HOST = "https://uploads.github.com"


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def authorization():
    result = subprocess.run(
        ["gh", "auth", "token", "--hostname", "github.com"],
        text=True, capture_output=True, check=False,
    )
    token = result.stdout.strip()
    if result.returncode or not token or any(c.isspace() for c in token):
        raise ValueError("GitHub credentials unavailable; authenticate gh for github.com first")
    return "Bearer " + token


def allowed_download(value, repository, redirect=False):
    u = urllib.parse.urlsplit(value)
    if (u.scheme != "https" or u.username or u.password or u.fragment
            or u.port not in (None, 443) or ".." in u.path or "\\" in u.path or len(value) > 8192):
        raise ValueError("Unexpected download origin")
    public = u.hostname == "github.com" and (u.path.startswith(f"/{repository}/releases/download/") or u.path.startswith(f"/{repository}/releases/latest/download/"))
    api = u.hostname == "api.github.com" and re.fullmatch(re.escape(("/repos/" + repository)) + r"/releases/assets/[0-9]+", u.path)
    cdn = redirect and u.hostname == "release-assets.githubusercontent.com" and u.path.startswith("/github-production-release-asset/")
    if not (cdn or ((public or api) and not u.query)):
        raise ValueError("Unexpected release download path")
    return value


class GitHubReleaseClient:
    def __init__(self, authorization, repository):
        if repository not in {"Acelith/Chronexus", "Acelith/Chronexus-Server"}:
            raise ValueError("Unexpected GitHub repository")
        self.repository = repository
        self.prefix = "/repos/" + repository
        self.authorization = authorization
        self.opener = urllib.request.build_opener(NoRedirect())

    def api(self, path, method="GET", body=None):
        if not (path == self.prefix or path.startswith(self.prefix + "/")) or ".." in path or "\\" in path:
            raise ValueError("Unexpected API path")
        request = urllib.request.Request(
            API_HOST + path, method=method,
            data=None if body is None else json.dumps(body).encode(),
            headers={"Authorization": self.authorization, "Accept": "application/vnd.github+json",
                     "Content-Type": "application/json", "X-GitHub-Api-Version": "2022-11-28"},
        )
        with self.opener.open(request, timeout=60) as response:
            data = response.read(2_000_001)
            if len(data) > 2_000_000:
                raise ValueError("API response too large")
            return json.loads(data or b"null")

    def upload(self, release, path):
        request = urllib.request.Request(
            f"{UPLOAD_HOST}{self.prefix}/releases/{int(release)}/assets?name={urllib.parse.quote(path.name)}",
            method="POST", data=path.read_bytes(),
            headers={"Authorization": self.authorization, "Content-Type": "application/octet-stream",
                     "Accept": "application/vnd.github+json"},
        )
        with self.opener.open(request, timeout=600) as response:
            return json.loads(response.read(1_000_000))

    def asset_api_url(self, asset):
        return f"{API_HOST}{self.prefix}/releases/assets/{int(asset)}"

    def allowed_download(self, value, redirect=False):
        return allowed_download(value, self.repository, redirect)

    def verify(self, url, expected, anonymous=False):
        url = self.allowed_download(url)
        digest = hashlib.sha256()
        size = 0
        for _ in range(6):
            # GitHub's API is needed for draft assets. Never attach credentials to
            # the website or signed CDN URL, including after an API redirect.
            auth = not anonymous and urllib.parse.urlsplit(url).hostname == "api.github.com"
            request = urllib.request.Request(url, headers={
                "Accept": "application/octet-stream",
                **({"Authorization": self.authorization} if auth else {}),
            })
            try:
                response = self.opener.open(request, timeout=600)
            except urllib.error.HTTPError as error:
                if error.code in (301, 302, 303, 307, 308):
                    target = urllib.parse.urljoin(url, error.headers["Location"])
                    error.close()
                    url = self.allowed_download(target, redirect=True)
                    continue
                raise
            with response:
                while block := response.read(1 << 20):
                    size += len(block)
                    if size > expected["size"]:
                        raise ValueError("Remote asset oversized")
                    digest.update(block)
            break
        else:
            raise ValueError("Too many asset redirects")
        if size != expected["size"] or digest.hexdigest() != expected["sha256"]:
            raise ValueError("Remote checksum mismatch")
