#!/usr/bin/env python3
"""HANDWRITTEN C fixture only: not a Rust or generated-code validation.

Tests the proposed V4 interface ABI plus the actual C runtime. Also round-trips
that handwritten C through Clang LLVM IR. Real compiler tests: scripts/validate.py.
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
EXPECTED = b'NACRE_MEMORY allocations=50004 frees=50004 live=0\n'


def check(cc, sanitize, llvm=False):
    source = (ROOT / 'src/runtime.c').read_text() + '\n' + (ROOT / 'tests/fixtures/interface_support.c').read_text()
    with tempfile.TemporaryDirectory(prefix='nacre-interface-support-') as tmp:
        c, exe, ir = Path(tmp)/'fixture.c', Path(tmp)/'fixture', Path(tmp)/'fixture.ll'
        c.write_text(source)
        flags = ['-std=c11', '-O1', '-g', '-DNACRE_MEMORY_STATS', '-Wall', '-Wextra',
                 '-Werror', '-pedantic-errors', '-Wno-unused-function']
        if sanitize:
            flags += ['-fsanitize=address,undefined', '-fno-sanitize-recover=all', '-fno-omit-frame-pointer']
            if sys.platform.startswith('linux'):
                flags += ['-fno-pie', '-no-pie']
        if llvm:
            subprocess.run([cc, *flags, '-S', '-emit-llvm', str(c), '-o', str(ir)],
                           check=True, capture_output=True, timeout=60)
            subprocess.run([cc, str(ir), '-o', str(exe)], check=True, capture_output=True, timeout=60)
            assert 'define ' in ir.read_text() and '@main(' in ir.read_text()
        else:
            subprocess.run([cc, *flags, str(c), '-o', str(exe)],
                           check=True, capture_output=True, timeout=60)
        env = dict(os.environ, ASAN_OPTIONS='detect_leaks=1:halt_on_error=1')
        run = subprocess.run([str(exe)], env=env, check=True, capture_output=True, timeout=30)
        assert run.stdout == b'', run.stdout
        assert run.stderr == EXPECTED, run.stderr
    return dict(compiler=cc, sanitizers=sanitize, llvm_roundtrip=llvm,
                allocations=50004, frees=50004, live_at_exit=0, status='passed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cc', action='append', default=[])
    args = parser.parse_args()
    compilers = args.cc or [c for c in ('gcc', 'clang') if shutil.which(c)]
    if not compilers:
        raise SystemExit('GCC/Clang manquant')
    results = []
    for cc in compilers:
        if not shutil.which(cc):
            raise SystemExit(f'Compilateur introuvable: {cc}')
        for sanitize in (False, True):
            results.append(check(cc, sanitize))
        if 'clang' in Path(cc).name:
            results.append(check(cc, False, llvm=True))
    print(json.dumps(dict(scope='actual runtime + HANDWRITTEN interface C fixture ONLY',
                          rust_tests_executed=False, generated_code_tested=False,
                          results=results), indent=2))


if __name__ == '__main__':
    try:
        main()
    except subprocess.CalledProcessError as e:
        sys.stderr.buffer.write(e.stderr or b'')
        raise
