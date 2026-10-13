#!/usr/bin/env python3
"""Packaging checks only. This is NOT a Rust compiler, type check or test run."""
import ast
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

def main():
    subprocess.run([sys.executable, str(ROOT/'scripts/generate_v2_tests.py'), '--check'], check=True)
    subprocess.run([sys.executable, str(ROOT/'scripts/generate_v3_tests.py'), '--check'], check=True)
    subprocess.run([sys.executable, str(ROOT/'scripts/generate_v4_tests.py'), '--check'], check=True)
    subprocess.run([sys.executable, str(ROOT/'scripts/generate_v5_tests.py'), '--check'], check=True)
    for path in (ROOT/'scripts').glob('*.py'):
        ast.parse(path.read_text(), filename=str(path))
    for path in (ROOT/'docs').glob('*.json'):
        json.loads(path.read_text())
    include_paths = 0
    rust_source_files = sorted((ROOT/'src').rglob('*.rs'))
    rust_test_files = sorted((ROOT/'tests').rglob('*.rs'))
    for path in rust_source_files + rust_test_files:
        text = path.read_text()
        for value in re.findall(r'include_str!\(\s*"([^"\n]+)"\s*\)', text):
            included = (path.parent/value).resolve()
            assert included.is_file(), (path, value)
            included.read_text()
            include_paths += 1
    counts = {}
    files = rust_test_files + rust_source_files
    for path in files:
        text = path.read_text()
        # Checked-in test macros close at column zero. Count their invocations,
        # not the #[test] declaration inside their unexpanded definition.
        without_macros = re.sub(r'macro_rules!\s+\w+\s*\{.*?^\}', '', text, flags=re.S|re.M)
        literal = len(re.findall(r'#\[test\]', without_macros))
        expanded = len(re.findall(r'^(?:accepts|rejects|case)!\(', without_macros, flags=re.M))
        if literal + expanded:
            counts[str(path.relative_to(ROOT))] = literal + expanded
    inventory = {'method':'static source inventory; not cargo test --list',
                 'rust_tests_executed':False, 'explicitly_ignored_llvm_tests':2, 'counts':counts, 'total':sum(counts.values())}
    (ROOT/'docs/test-inventory.json').write_text(json.dumps(inventory, indent=2)+'\n')
    print(json.dumps({'scope':'packaging checks only', 'status':'passed',
                      'include_str_paths':include_paths,
                      'declared_rust_tests_not_executed':inventory['total']}, indent=2))

if __name__ == '__main__':
    main()
