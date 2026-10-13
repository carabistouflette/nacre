#!/usr/bin/env python3
"""Run the real Nacre programs listed in catalog.json."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
CATALOG = Path(__file__).with_name("catalog.json")


def resolve_compiler(configured):
    requested = configured or os.environ.get("NACRE_BIN")
    if requested:
        candidate = Path(requested)
        if not candidate.is_absolute():
            repository_candidate = ROOT / candidate
            if repository_candidate.is_file():
                candidate = repository_candidate
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)
        located = shutil.which(requested)
        if located:
            return located
        raise SystemExit(f"Nacre compiler is not executable: {requested}")

    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    suffix = ".exe" if os.name == "nt" else ""
    for profile in ("debug", "release"):
        candidate = target / profile / f"nacre{suffix}"
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)

    located = shutil.which("nacre")
    if located:
        return located
    raise SystemExit("Nacre compiler not found; build it or set NACRE_BIN.")


def load_catalog():
    catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    examples = catalog.get("examples")
    if not isinstance(examples, list):
        raise SystemExit(f"Invalid catalog: {CATALOG} must contain an examples array")
    required = {"category", "source", "stdin", "args", "expected_status", "meaningful_outputs"}
    for index, example in enumerate(examples):
        missing = required - example.keys()
        if missing:
            raise SystemExit(f"Catalog entry {index + 1} is missing: {', '.join(sorted(missing))}")
    return examples


def has_meaningful_output(stdout, marker):
    if marker.lstrip("-").isdigit():
        return marker in stdout.splitlines()
    return marker in stdout


def run_example(compiler, example):
    source = Path(example["source"])
    if source.is_absolute():
        raise SystemExit(f"Catalog source must be repository-relative: {source}")
    source_path = ROOT / source
    if not source_path.exists():
        print(f"FAIL {example['source']}: source does not exist", file=sys.stderr)
        return False

    command = [compiler, "run", source.as_posix()]
    if example["args"]:
        command.extend(["--", *example["args"]])
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            input=example["stdin"],
            text=True,
            capture_output=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        print(f"FAIL {example['source']}: {error}", file=sys.stderr)
        return False

    missing = [marker for marker in example["meaningful_outputs"] if not has_meaningful_output(result.stdout, marker)]
    passed = result.returncode == example["expected_status"] and not missing
    label = "PASS" if passed else "FAIL"
    print(f"{label} {example['category']}: {example['source']} (status {result.returncode})")
    if not passed:
        if result.returncode != example["expected_status"]:
            print(f"  expected status: {example['expected_status']}", file=sys.stderr)
        if missing:
            print(f"  missing output: {missing!r}", file=sys.stderr)
        if result.stdout:
            print("  stdout:", file=sys.stderr)
            print(result.stdout, file=sys.stderr, end="" if result.stdout.endswith("\n") else "\n")
        if result.stderr:
            print("  stderr:", file=sys.stderr)
            print(result.stderr, file=sys.stderr, end="" if result.stderr.endswith("\n") else "\n")
    return passed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", help="Nacre compiler path; defaults to NACRE_BIN, a local Cargo build, or PATH")
    parser.add_argument("--category", help="run only one catalog category")
    parser.add_argument("--list", action="store_true", help="list catalog categories and example counts")
    args = parser.parse_args()

    examples = load_catalog()
    if args.list:
        counts = {}
        for example in examples:
            counts[example["category"]] = counts.get(example["category"], 0) + 1
        for category, count in sorted(counts.items()):
            print(f"{category}: {count}")
        return 0

    selected = [example for example in examples if not args.category or example["category"] == args.category]
    if not selected:
        parser.error(f"no catalog entries for category {args.category!r}")
    compiler = resolve_compiler(args.compiler)
    passed = sum(run_example(compiler, example) for example in selected)
    failed = len(selected) - passed
    print(f"Examples passed: {passed}/{len(selected)}")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
