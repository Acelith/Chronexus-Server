#!/usr/bin/env python3
"""Push a multi-platform OCI image archive to the Forgejo container registry on github.com.

usage: push-image.py ARCHIVE.tar REPOSITORY TAG [TAG...]
  ARCHIVE.tar  from `docker buildx build --platform ... --output type=oci,dest=ARCHIVE.tar`
  REPOSITORY   e.g. acelith/chronexus-server

Why: `docker push` from Docker Desktop timed out on large layers ("timeout awaiting response
headers") while direct HTTPS uploads from the host work. Blobs are uploaded in 8 MiB chunks
through the standard OCI distribution API; existing blobs are skipped; manifests are pushed by
digest, then the index under each tag. Credentials come from the configured Git credential
helper, in memory only, for github.com only.
"""
import base64, hashlib, json, os, subprocess, sys, tarfile, urllib.error, urllib.parse, urllib.request

HOST = "github.com"
CHUNK = 8 << 20


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


opener = urllib.request.build_opener(NoRedirect())


def basic():
    result = subprocess.run(["git", "-c", "credential.interactive=never", "credential", "fill"],
                            input=f"url=https://{HOST}\n\n", text=True, capture_output=True,
                            env={**os.environ, "GIT_TERMINAL_PROMPT": "0"})
    f = dict(l.split("=", 1) for l in result.stdout.splitlines() if "=" in l)
    if result.returncode or f.get("host") != HOST or not f.get("password"):
        raise SystemExit("Git credentials for github.com are unavailable.")
    return "Basic " + base64.b64encode(f"{f['username']}:{f['password']}".encode()).decode()


def token(repo):
    scope = urllib.parse.quote(f"repository:{repo}:pull,push")
    req = urllib.request.Request(f"https://{HOST}/v2/token?service=container_registry&scope={scope}",
                                 headers={"Authorization": basic()})
    with opener.open(req, timeout=60) as r:
        return "Bearer " + json.load(r)["token"]


def request(auth, method, url, data=None, headers=None, ok=(200, 201, 202, 204)):
    if not url.startswith("http"):
        url = f"https://{HOST}{url}"
    if urllib.parse.urlsplit(url).netloc != HOST:
        raise SystemExit("Unexpected registry host.")
    req = urllib.request.Request(url, data=data, method=method, headers={"Authorization": auth, **(headers or {})})
    try:
        with opener.open(req, timeout=600) as r:
            return r.status, dict(r.headers), r.read()
    except urllib.error.HTTPError as e:
        if e.code in ok:
            return e.code, dict(e.headers), e.read()
        raise SystemExit(f"{method} {urllib.parse.urlsplit(url).path}: HTTP {e.code} {e.read()[:200]!r}")


def push_blob(auth, repo, digest, blob):
    status, _, _ = request(auth, "HEAD", f"/v2/{repo}/blobs/{digest}", ok=(200, 404))
    if status == 200:
        return "exists"
    _, headers, _ = request(auth, "POST", f"/v2/{repo}/blobs/uploads/")
    location = headers["Location"]
    for start in range(0, len(blob), CHUNK):
        part = blob[start:start + CHUNK]
        _, headers, _ = request(auth, "PATCH", location, part, {
            "Content-Type": "application/octet-stream",
            "Content-Range": f"{start}-{start + len(part) - 1}", "Content-Length": str(len(part))})
        location = headers["Location"]
    sep = "&" if "?" in location else "?"
    request(auth, "PUT", f"{location}{sep}digest={urllib.parse.quote(digest)}", b"", {"Content-Length": "0"})
    return f"uploaded {len(blob) >> 20} MiB"


def main():
    archive, repo, *tags = sys.argv[1:]
    with tarfile.open(archive) as tar:
        blob = lambda d: tar.extractfile(f"blobs/sha256/{d.split(':', 1)[1]}").read()
        index_desc = json.loads(tar.extractfile("index.json").read())["manifests"][0]
        index = blob(index_desc["digest"])
        auth = token(repo)
        for desc in json.loads(index)["manifests"]:
            manifest = blob(desc["digest"])
            parsed = json.loads(manifest)
            for layer in [parsed["config"], *parsed["layers"]]:
                data = blob(layer["digest"])
                if "sha256:" + hashlib.sha256(data).hexdigest() != layer["digest"]:
                    raise SystemExit("Archive blob does not match its digest.")
                print(desc.get("platform", {}).get("architecture", "?"), layer["digest"][:19], push_blob(auth, repo, layer["digest"], data))
            request(auth, "PUT", f"/v2/{repo}/manifests/{desc['digest']}", manifest,
                    {"Content-Type": desc["mediaType"]})
        for tag in tags:
            request(auth, "PUT", f"/v2/{repo}/manifests/{tag}", index, {"Content-Type": index_desc["mediaType"]})
            print("tagged", tag, index_desc["digest"])


if __name__ == "__main__":
    main()
