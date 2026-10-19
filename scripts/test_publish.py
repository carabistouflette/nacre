#!/usr/bin/env python3
"""Exercise publication against real disposable Git repositories and a local bare remote."""
import hashlib
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
PUBLISH = ROOT / "scripts/publish.py"


def git(directory: Path, *arguments: str) -> bytes:
    return subprocess.run(
        ["git", "-C", str(directory), *arguments], check=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    ).stdout


def configure(directory: Path) -> None:
    git(directory, "config", "user.name", "Publication Test")
    git(directory, "config", "user.email", "publication@example.invalid")
    git(directory, "config", "commit.gpgsign", "false")
    git(directory, "config", "core.hooksPath", "/dev/null")


class PublishingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.seed_temp = tempfile.TemporaryDirectory(prefix="nacre-publish-seed-")
        cls.seed = Path(cls.seed_temp.name)
        with subprocess.Popen(["git", "-C", str(ROOT), "archive", "HEAD"], stdout=subprocess.PIPE) as archive:
            with tarfile.open(fileobj=archive.stdout, mode="r|") as files:
                files.extractall(cls.seed, filter="data")
            if archive.wait() != 0:
                raise RuntimeError("Cannot export publication test sources")
        (cls.seed / "publication-remove.txt").write_text("old publication data\n")
        git(cls.seed, "init", "--initial-branch=quality-test")
        configure(cls.seed)
        git(cls.seed, "add", "--all")
        git(cls.seed, "commit", "-m", "Seed actual curated sources")

    @classmethod
    def tearDownClass(cls):
        cls.seed_temp.cleanup()

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="nacre-publish-test-")
        self.addCleanup(self.temporary.cleanup)
        base = Path(self.temporary.name)
        self.work = base / "work"
        self.remote = base / "remote.git"
        git(base, "clone", "--quiet", "--shared", str(self.seed), str(self.work))
        git(base, "init", "--bare", str(self.remote))
        configure(self.work)
        git(self.work, "remote", "set-url", "origin", str(self.remote))
        self.payload = self.work / "publication-data.txt"
        self.payload.write_bytes(b"staged release bytes\n")
        git(self.work, "add", "--", self.payload.name)
        self.payload.write_bytes(b"unstaged private bytes\n")
        self.inventory = self.work / "docs/test-inventory.json"
        self.inventory.write_bytes(self.inventory.read_bytes() + b"\n")
        self.original_inventory = self.inventory.read_bytes()
        self.original_manifest = (self.work / "MANIFEST.sha256").read_bytes()
        self.original_tree = git(self.work, "write-tree")
        self.original_head = git(self.work, "rev-parse", "HEAD")

    def publish(self, *arguments: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(PUBLISH), *arguments], cwd=self.work,
            text=True, capture_output=True,
        )

    def assert_not_published(self):
        self.assertEqual(git(self.work, "rev-parse", "HEAD"), self.original_head)
        self.assertEqual(git(self.work, "write-tree"), self.original_tree)
        self.assertEqual((self.work / "MANIFEST.sha256").read_bytes(), self.original_manifest)
        self.assertEqual(self.inventory.read_bytes(), self.original_inventory)
        self.assertEqual(self.payload.read_bytes(), b"unstaged private bytes\n")
        self.assertEqual(git(self.remote, "for-each-ref", "--format=%(refname)"), b"")

    def test_push_hashes_staged_bytes_and_removes_deleted_entries(self):
        git(self.work, "rm", "--", "publication-remove.txt")
        result = self.publish("--message", "Publish selected sources")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        head = git(self.work, "rev-parse", "HEAD")
        self.assertNotEqual(head, self.original_head)
        self.assertEqual(git(self.remote, "rev-parse", "refs/heads/quality-test"), head)
        self.assertEqual(git(self.remote, "show", "quality-test:publication-data.txt"), b"staged release bytes\n")
        manifest = git(self.remote, "show", "quality-test:MANIFEST.sha256").decode()
        entries = {}
        for row in manifest.splitlines():
            digest, path = row.split("  ", 1)
            entries[path] = digest
        self.assertEqual(entries[self.payload.name], hashlib.sha256(b"staged release bytes\n").hexdigest())
        self.assertNotIn("publication-remove.txt", entries)
        self.assertEqual(
            set(entries),
            set(git(self.work, "ls-tree", "-r", "--name-only", "HEAD").decode().splitlines()) - {"MANIFEST.sha256"},
        )
        self.assertEqual(self.payload.read_bytes(), b"unstaged private bytes\n")
        self.assertEqual(self.inventory.read_bytes(), self.original_inventory)

    def test_check_does_not_change_index_worktree_or_remote(self):
        result = self.publish("--check")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_not_published()

    def test_failed_audit_does_not_mutate_publication(self):
        invalid = self.work / "docs/publication-invalid.json"
        invalid.write_text("{invalid JSON")
        git(self.work, "add", "--", str(invalid))
        self.original_tree = git(self.work, "write-tree")
        result = self.publish("--message", "Must not publish invalid sources")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Publication stopped", result.stderr)
        self.assert_not_published()

    def test_pre_commit_hook_cannot_push_unhashed_files(self):
        hooks = self.work / "test-hooks"
        hooks.mkdir()
        hook = hooks / "pre-commit"
        hook.write_text(
            "#!/bin/sh\n"
            "printf 'hook-added content\\n' > hook-added.txt\n"
            "git add -- hook-added.txt\n"
        )
        hook.chmod(0o755)
        git(self.work, "config", "core.hooksPath", str(hooks))

        result = self.publish("--message", "Publish only audited content")
        self.assertEqual(
            git(self.remote, "for-each-ref", "--format=%(refname)"), b"",
            result.stdout + result.stderr,
        )
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("audited tree", result.stderr)
        self.assertNotEqual(git(self.work, "rev-parse", "HEAD"), self.original_head)
        committed = git(self.work, "ls-tree", "-r", "--name-only", "HEAD").decode()
        self.assertIn("hook-added.txt", committed)
        manifest = git(self.work, "show", "HEAD:MANIFEST.sha256").decode()
        self.assertNotIn("hook-added.txt", manifest)

    def test_graphify_artifacts_are_rejected_even_when_force_staged(self):
        artifact = self.work / "graphify-out/graph.json"
        artifact.parent.mkdir()
        artifact.write_text('{"private": true}')
        git(self.work, "add", "--force", "--", str(artifact))
        self.original_tree = git(self.work, "write-tree")
        result = self.publish("--message", "Must not publish analysis artifacts")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Graphify artifacts must stay outside Git", result.stderr)
        self.assert_not_published()

    def test_main_branch_cannot_be_published(self):
        git(self.work, "checkout", "-b", "main")
        result = self.publish("--message", "Must not publish main")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("feature branch", result.stderr)
        self.assert_not_published()

    def test_unstaged_manifest_edits_are_not_overwritten(self):
        self.original_manifest += b"private manual manifest edit\n"
        (self.work / "MANIFEST.sha256").write_bytes(self.original_manifest)
        result = self.publish("--message", "Must not overwrite a local manifest")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("MANIFEST.sha256 has unstaged edits", result.stderr)
        self.assert_not_published()


if __name__ == "__main__":
    unittest.main()
