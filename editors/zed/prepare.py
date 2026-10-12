#!/usr/bin/env python3
"""Prepare a relocatable local Zed dev extension without publishing a Git repo."""
import json
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
GRAMMAR = HERE.parent / "tree-sitter-nacre"
OUTPUT = HERE / "dist"
REPOSITORY = OUTPUT / "grammar-source"
EXTENSION = OUTPUT / "nacre"


def git(*args):
    return subprocess.check_output(["git", "-C", str(REPOSITORY), *args], text=True).strip()


def main():
    parser = GRAMMAR / "src" / "parser.c"
    if not parser.is_file():
        raise SystemExit("Generate the parser first: cd editors/tree-sitter-nacre && npm ci && npm run generate")
    REPOSITORY.mkdir(parents=True, exist_ok=True)
    EXTENSION.mkdir(parents=True, exist_ok=True)
    shutil.copytree(GRAMMAR / "src", REPOSITORY / "src", dirs_exist_ok=True)
    for name in ["grammar.js", "tree-sitter.json"]:
        shutil.copy2(GRAMMAR / name, REPOSITORY / name)
    if not (REPOSITORY / ".git").exists():
        git("init", "--quiet")
    git("add", "src", "grammar.js", "tree-sitter.json")
    staged = subprocess.run(
        ["git", "-C", str(REPOSITORY), "diff", "--cached", "--quiet"], check=False
    )
    if staged.returncode not in (0, 1):
        raise SystemExit("Could not inspect the generated grammar snapshot.")
    if staged.returncode == 1:
        subprocess.run([
            "git", "-C", str(REPOSITORY), "-c", "user.name=Nacre Editor Build",
            "-c", "user.email=editor-build@localhost", "-c", "commit.gpgsign=false",
            "commit", "--quiet", "-m", "Local Nacre parser snapshot",
        ], check=True)
    rev = git("rev-parse", "HEAD")
    shutil.copy2(HERE / "Cargo.toml", EXTENSION / "Cargo.toml")
    shutil.copy2(HERE / "Cargo.lock", EXTENSION / "Cargo.lock")
    shutil.copytree(HERE / "src", EXTENSION / "src", dirs_exist_ok=True)
    shutil.copytree(HERE / "languages", EXTENSION / "languages", dirs_exist_ok=True)
    (EXTENSION / "snippets").mkdir(exist_ok=True)
    shutil.copy2(
        HERE.parent / "vscode/snippets/nacre.json",
        EXTENSION / "snippets/nacre.json",
    )
    # JSON strings are valid TOML basic strings, including escaped paths.
    manifest = '\n'.join([
        'id = "nacre"', 'name = "Nacre"', 'version = "0.6.0"',
        'schema_version = 1', 'authors = ["Nacre contributors"]',
        'description = "Nacre highlighting and language server support"',
        'snippets = ["./snippets/nacre.json"]',
        '', '[language_servers.nacre]',
        'name = "Nacre Language Server"', 'languages = ["Nacre"]',
        'language_ids = { "Nacre" = "nacre" }',
        '', '[grammars.nacre]',
        f'repository = {json.dumps(REPOSITORY.as_uri())}', f'rev = "{rev}"', '',
    ])
    (EXTENSION / "extension.toml").write_text(manifest, encoding="utf-8")
    print(f"In Zed, run 'zed: install dev extension' and select:\n{EXTENSION}")
    print("Keep the dist directory in place; rerun this script after moving the project or changing the grammar.")


if __name__ == "__main__":
    main()
