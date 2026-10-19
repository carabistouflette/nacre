#!/usr/bin/env python3
"""Hash and audit staged sources, then commit and push the current feature branch.

Stage explicit owned paths first. --check previews the same publication without
changing the worktree/index, committing, or pushing. Audits run only in a temporary
export because audit_package.py writes its source inventory.

Commit hooks remain enabled; the resulting commit tree and parent are verified
before pushing. If a hook changes the audited commit, the local commit is left
unpublished for inspection.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile


MANIFEST = "MANIFEST.sha256"


def git(root: Path, *arguments: str) -> bytes:
    return subprocess.run(
        ["git", "-C", str(root), *arguments], check=True, stdout=subprocess.PIPE,
    ).stdout


def export_tree(root: Path, tree: str, destination: Path) -> bytes:
    """Export exact Git blobs, without checkout filters or unstaged content."""
    entries = []
    for entry in git(root, "ls-tree", "-rz", "--full-tree", tree).split(b"\0"):
        if not entry:
            continue
        metadata, raw_path = entry.split(b"\t", 1)
        mode, kind, object_id = metadata.split()
        path = raw_path.decode("utf-8")
        if any(part == "graphify-out" or part.startswith(".graphify") for part in Path(path).parts):
            raise ValueError(f"Graphify artifacts must stay outside Git: {path}")
        if mode not in (b"100644", b"100755") or kind != b"blob":
            raise ValueError(f"Publication requires regular source files: {path}")
        if any(character in path for character in "\\\r\n"):
            raise ValueError(f"Unsupported checksum filename: {path!r}")
        entries.append((path, mode, object_id))

    rows = []
    with subprocess.Popen(
        ["git", "-C", str(root), "cat-file", "--batch"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
    ) as objects:
        assert objects.stdin is not None and objects.stdout is not None
        for path, mode, object_id in entries:
            objects.stdin.write(object_id + b"\n")
            objects.stdin.flush()
            header = objects.stdout.readline().split()
            if len(header) != 3 or header[:2] != [object_id, b"blob"]:
                raise RuntimeError(f"Cannot read staged Git blob: {path}")
            remaining = int(header[2])
            digest = hashlib.sha256()
            target = destination / path
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("wb") as output:
                while remaining:
                    chunk = objects.stdout.read(min(remaining, 262144))
                    if not chunk:
                        raise RuntimeError(f"Truncated staged Git blob: {path}")
                    output.write(chunk)
                    digest.update(chunk)
                    remaining -= len(chunk)
            if objects.stdout.read(1) != b"\n":
                raise RuntimeError(f"Invalid Git blob boundary: {path}")
            target.chmod(int(mode, 8) & 0o777)
            if path != MANIFEST:
                rows.append((path, digest.hexdigest()))
        objects.stdin.close()
        if objects.wait() != 0:
            raise RuntimeError("Git blob export failed")
    return "".join(f"{digest}  {path}\n" for path, digest in sorted(rows)).encode("utf-8")


def publish(check: bool, message: str | None, no_push: bool) -> None:
    root = Path(git(Path.cwd(), "rev-parse", "--show-toplevel").decode().strip())
    branch = git(root, "branch", "--show-current").decode().strip()
    starting_head = git(root, "rev-parse", "HEAD").decode().strip()
    if not check and (not branch or branch in {"main", "master"}):
        raise ValueError("Publish from a feature branch, not main/master or detached HEAD")
    tree = git(root, "write-tree").decode().strip()
    if not check and tree == git(root, "rev-parse", "HEAD^{tree}").decode().strip():
        raise ValueError("No staged changes; stage explicit owned paths first")

    with tempfile.TemporaryDirectory(prefix="nacre-publish-") as temporary:
        export = Path(temporary)
        manifest = export_tree(root, tree, export)
        original_manifest = (export / MANIFEST).read_bytes()
        (export / MANIFEST).write_bytes(manifest)
        subprocess.run(["sha256sum", "--check", "--quiet", MANIFEST], cwd=export, check=True)
        subprocess.run([sys.executable, "scripts/audit_package.py"], cwd=export, check=True)
        print(f"Publication checks passed for {len(manifest.splitlines())} indexed files.", flush=True)

    if check:
        print("Check only: no worktree/index changes, commit, or push.")
        return
    if git(root, "write-tree").decode().strip() != tree:
        raise ValueError("Staged content changed during checks; review it and run again")
    if git(root, "rev-parse", "HEAD").decode().strip() != starting_head:
        raise ValueError("HEAD changed during checks; review it and run again")
    if (root / MANIFEST).read_bytes() != original_manifest:
        raise ValueError("MANIFEST.sha256 has unstaged edits; stage or preserve them explicitly")
    if git(root, "branch", "--show-current").decode().strip() != branch:
        raise ValueError("Branch changed during checks; run again on the intended feature branch")

    (root / MANIFEST).write_bytes(manifest)
    subprocess.run(["git", "-C", str(root), "add", "--", MANIFEST], check=True)
    expected_commit_tree = git(root, "write-tree").decode().strip()
    subprocess.run(["git", "-C", str(root), "commit", "-m", message], check=True)
    commit = git(root, "rev-parse", "HEAD").decode().strip()
    committed_tree = git(root, "rev-parse", f"{commit}^{{tree}}").decode().strip()
    committed_parent = git(root, "rev-parse", f"{commit}^").decode().strip()
    if committed_tree != expected_commit_tree or committed_parent != starting_head:
        raise ValueError(
            "Commit hooks changed the audited tree or parent; the local commit was "
            "created but will not be pushed"
        )
    if git(root, "branch", "--show-current").decode().strip() != branch:
        raise ValueError(
            "Branch changed during commit; the local commit was created but will not be pushed"
        )
    if not no_push:
        subprocess.run([
            "git", "-C", str(root), "push", "--set-upstream", "origin",
            f"{commit}:refs/heads/{branch}",
        ], check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--check", action="store_true", help="audit staged publication without mutations")
    action.add_argument("--message", help="commit message for the staged publication")
    parser.add_argument("--no-push", action="store_true", help="commit locally without pushing")
    args = parser.parse_args()
    if args.message is not None and not args.message.strip():
        parser.error("--message must not be empty")
    try:
        publish(args.check, args.message, args.no_push)
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Publication stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
