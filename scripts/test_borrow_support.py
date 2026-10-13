#!/usr/bin/env python3
"""Execute the exact C runtime plus a HANDWRITTEN borrowed-pointer fixture.

This is NOT a Rust compiler test and NOT a generated-code test.
For actual end-to-end testing, use scripts/validate.py on a machine with Cargo.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]

def check(cc, sanitizers):
    source = (ROOT / 'src/runtime.c').read_text() + '\n' + (ROOT / 'tests/fixtures/borrow_support.c').read_text()
    with tempfile.TemporaryDirectory(prefix='nacre-borrow-support-') as tmp:
        c_file, exe = Path(tmp) / 'fixture.c', Path(tmp) / 'fixture'
        c_file.write_text(source, encoding='utf-8')
        flags = ['-std=c11', '-O1', '-g', '-DNACRE_MEMORY_STATS']
        if sanitizers:
            flags += ['-fsanitize=address,undefined', '-fno-sanitize-recover=all', '-fno-omit-frame-pointer']
            # Linux-only: avoid sanitizer/PIE address-space collisions in small
            # containers. This does not disable any sanitizer checks.
            if sys.platform.startswith('linux'):
                flags += ['-fno-pie', '-no-pie']
        subprocess.run([cc, *flags, str(c_file), '-o', str(exe)], check=True, capture_output=True, timeout=60)
        env = dict(os.environ, ASAN_OPTIONS='detect_leaks=1:halt_on_error=1')
        result = subprocess.run([str(exe)], capture_output=True, env=env, check=True, timeout=30)
        assert result.stdout == b'', result.stdout
        assert result.stderr == b'NACRE_MEMORY allocations=20001 frees=20001 live=0\n', result.stderr
    return {'compiler': cc, 'fixture': 'handwritten C, not compiler output',
            'sanitizers': sanitizers, 'allocations': 20001, 'frees': 20001,
            'live_at_exit': 0, 'status': 'passed'}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cc', action='append', default=[])
    args = parser.parse_args()
    compilers = args.cc or [c for c in ('gcc', 'clang') if shutil.which(c)]
    if not compilers:
        raise SystemExit('GCC/Clang is required')
    for cc in compilers:
        if not shutil.which(cc):
            raise SystemExit(f'C compiler not found: {cc}')
    results = [check(cc, sanitizers) for cc in compilers for sanitizers in (False, True)]
    print(json.dumps({'scope': 'C runtime and handwritten borrowed-pointer fixture only',
                      'rust_tests_executed': False, 'results': results}, indent=2))

if __name__ == '__main__':
    main()
