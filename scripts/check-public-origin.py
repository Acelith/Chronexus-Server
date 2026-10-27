#!/usr/bin/env python3
"""Audit current files and reachable Git objects without storing the retired host."""
import argparse
import hashlib
import re
import subprocess
import urllib.parse
from pathlib import Path

REPOSITORY = "Acelith/Chronexus-Server"
RETIRED_HOST_SHA256 = "bf3151d00dca5941c8f22a8ca1b20ada0af2d6fef8d051ee2d4cba83662f2f56"
HOST = re.compile(rb"(?<![a-z0-9.-])(?:[a-z0-9-]+\.)+[a-z]{2,63}(?![a-z0-9.-])")


def retired(data, fingerprint=RETIRED_HOST_SHA256):
    for _ in range(3):
        if any(hashlib.sha256(match[0]).hexdigest() == fingerprint for match in HOST.finditer(data.lower())):
            return True
        decoded = urllib.parse.unquote_to_bytes(data)
        if decoded == data:
            break
        data = decoded
    return False


def git(*args):
    return subprocess.check_output(["git", *args])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", action="store_true")
    parser.add_argument("--origin", action="store_true")
    args = parser.parse_args()
    failures = []
    files = 0
    bare = git("rev-parse", "--is-bare-repository").strip() == b"true"
    for raw in (b"" if bare else git("ls-files", "-z")).split(b"\0"):
        if raw and (path := Path(raw.decode())).is_file():
            files += 1
            if retired(path.read_bytes()):
                failures.append("file " + str(path))
    objects = 0
    if args.history:
        identifiers = {line.split(b" ", 1)[0] for line in git("rev-list", "--objects", "--all").splitlines()}
        with subprocess.Popen(["git", "cat-file", "--batch"], stdin=subprocess.PIPE, stdout=subprocess.PIPE) as process:
            for identifier in sorted(identifiers):
                process.stdin.write(identifier + b"\n")
                process.stdin.flush()
                header = process.stdout.readline().split()
                data = process.stdout.read(int(header[2]))
                process.stdout.read(1)
                objects += 1
                if retired(data):
                    failures.append("object " + identifier.decode() + " (" + header[1].decode() + ")")
            process.stdin.close()
            if process.wait():
                raise SystemExit("Git object audit failed")
    if args.origin:
        expected = "https://github.com/" + REPOSITORY + ".git"
        fetch = git("remote", "get-url", "origin").decode().strip()
        pushes = git("remote", "get-url", "--push", "--all", "origin").decode().splitlines()
        if fetch.removesuffix(".git") != expected.removesuffix(".git") or len(pushes) != 1 or pushes[0].removesuffix(".git") != expected.removesuffix(".git"):
            failures.append("origin does not exclusively target the canonical GitHub repository")
    print(f"Public-origin audit: {files} current files, {objects} reachable objects, {len(failures)} failures")
    for failure in failures:
        print(failure)
    raise SystemExit(bool(failures))


if __name__ == "__main__":
    main()
