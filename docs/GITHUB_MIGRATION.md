# GitHub repository migration

GitHub is the canonical home of both projects:

- https://github.com/Acelith/Chronexus
- https://github.com/Acelith/Chronexus-Server

The owner authorized a one-time history rewrite on 27 October 2026. It replaces
retired hosting references in every reachable file revision and commit/tag message.
Branches, annotated tag names, authors, timestamps and campaign compatibility are
preserved; affected commit and tag object IDs change. Private recovery bundles and
the old-to-new object maps are kept outside both repositories and are never pushed.

Existing clones must be freshly cloned from GitHub, or carefully transplanted onto
the rewritten history. Merging old history can bring the retired references back.
Normal future contributions must not rewrite history. Only `main` and release tags
are published; local agent checkpoints and recovery references are not public refs.

Run the read-only audit before publication:

```sh
python3 scripts/check-public-origin.py --history --origin
```

The audit checks current tracked files, reachable blobs/trees/commits/tags, encoded
hostnames, and every fetch/push origin. It stores only a retired-host fingerprint.
GitHub Actions checks complete fetched history, preventing an old-history merge
from quietly reintroducing the retired host.

## Releases, downloads and documentation

Release descriptions and the stable updater manifest move to GitHub URLs. Existing
installer bytes and signatures are historical builds and are not rewritten or
represented as newly compiled packages. Their build identifiers predate the
history migration. A new binary release still needs its own qualified build.

Server release publication uses `scripts/publish-release.py`, GitHub drafts and
verified anonymous downloads. `scripts/push-image.py` targets GHCR only; it requires
an explicitly configured GitHub credential with package publication permission.
No GHCR image publication is implied by moving repository source or release binaries.

The handbook lives in `docs/wiki/` with ordinary Markdown links. This is usable
directly on GitHub without initializing the optional hosted Wiki. Server containers
build locally until GHCR images are actually published and verified.

## Limits of history removal

The audit proves removal from reachable repository history and published metadata.
It cannot erase previously downloaded clones, installers or GitHub's cached views
of unreachable object IDs. GitHub documents those separate retention limits in
[Removing sensitive data from a repository](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository).

On 27 October 2026, fresh public mirror clones passed the complete reachable-object
scan and Git integrity check. GitHub still resolved the previous main commit by SHA;
that cached-object retention is an observed outstanding limitation. An offline support
request was prepared privately, without contacting GitHub or deleting either repository.
