#!/usr/bin/env python3
"""End-to-end checks using the actual Rust compiler and its generated programs.

Requires Cargo and GCC/Clang. No silent fallback to a reference implementation.
Optional sanitizers are applied to generated C, not to handwritten substitutes.
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
EXAMPLES = {
    "borrowing": "42\n84\n43\n",
    "sums": "42\n0\n",
    "reborrowing": "1\n2\n2\n",
    "borrowed_fields": "Shared counter\n42\n43\n",
    "borrowed_loop": "8\n",
    "readonly_composition": "120\n130\n",
    "counter": "42\n",
    "composition": "My car\n150\n",
    "ownership": "42\n42\n7\n",
    "field_initializers": "Settings\ntrue\n200\n",
    "static_methods": "36\n49\n",
    "object_scopes": "0\n2\n100\n",
    "ownership_loop": "13\n",
    "definite_initialization": "Initialized through the first branch.\nInitialized through the second branch.\n",
    "explicit_drop": "150\n150\n",
    "ownership_continue": "40\n",
    "ownership_break": "7\n",
}
EXAMPLE_PATHS = {
    "borrowing": "ownership/borrowing.ncr",
    "sums": "language/sum_types.ncr",
    "reborrowing": "ownership/reborrowing.ncr",
    "borrowed_fields": "ownership/borrowed_fields.ncr",
    "borrowed_loop": "ownership/borrowed_loop.ncr",
    "readonly_composition": "ownership/readonly_composition.ncr",
    "counter": "language/counter.ncr",
    "composition": "ownership/composition.ncr",
    "ownership": "ownership/ownership.ncr",
    "field_initializers": "language/field_initialization.ncr",
    "static_methods": "language/static_methods.ncr",
    "object_scopes": "ownership/scope_cleanup.ncr",
    "ownership_loop": "ownership/loop_cleanup.ncr",
    "definite_initialization": "language/definite_initialization.ncr",
    "explicit_drop": "ownership/explicit_drop.ncr",
    "ownership_continue": "ownership/continue_cleanup.ncr",
    "ownership_break": "ownership/break_cleanup.ncr",
    "interfaces": "language/interfaces.ncr",
    "interface_borrowing": "ownership/interface_borrowing.ncr",
    "generic_box": "language/generic_box.ncr",
    "generic_composition": "ownership/generic_composition.ncr",
    "generic_interfaces": "language/generic_interfaces.ncr",
    "interface_composition": "language/interface_composition.ncr",
    "generic_view": "ownership/generic_view.ncr",
    "collections": "collections/list_operations.ncr",
    "owned_list": "collections/owned_list.ncr",
    "owned_tree": "ownership/owned_tree.ncr",
    "dynamic_text": "text/dynamic_text.ncr",
}


def example_source(name):
    return ROOT / "examples" / EXAMPLE_PATHS[name]


def command(args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)

def assert_memory_balanced(stderr, name):
    fields = stderr.decode('ascii').split()
    assert len(fields) == 4 and fields[0] == 'NACRE_MEMORY', (name, stderr)
    allocations = int(fields[1].removeprefix('allocations='), 10)
    frees = int(fields[2].removeprefix('frees='), 10)
    assert allocations == frees and fields[3] == 'live=0', (name, stderr)
    return allocations, frees

def exercise_native(nacre, source, name, expected, cc, directory, env, sanitizers):
    c_file = directory / f'{name}.c'
    exe = directory / name
    command([str(nacre), 'emit-c', str(source), '-o', str(c_file)], capture_output=True)
    flags = ['-std=c11', '-O1', '-g', '-pthread', '-DNACRE_MEMORY_STATS']
    if sanitizers:
        flags += ['-fsanitize=address,undefined', '-fno-sanitize-recover=all', '-fno-omit-frame-pointer']
        if sys.platform.startswith('linux'):
            flags += ['-fno-pie', '-no-pie']
    command([cc, *flags, str(c_file), '-lm', '-o', str(exe)], capture_output=True)
    run_env = dict(env, ASAN_OPTIONS='detect_leaks=1:halt_on_error=1')
    result = command([str(exe)], capture_output=True, env=run_env, timeout=20)
    assert result.stdout == expected.encode('utf-8'), (name, result.stdout)
    allocations, frees = assert_memory_balanced(result.stderr, name)
    return {'compiler': cc, 'case': name, 'allocations': allocations,
            'frees': frees, 'live_at_exit': 0,
            'sanitizers': sanitizers, 'status': 'passed'}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cc', action='append', default=[])
    parser.add_argument('--sanitizers', action='store_true')
    parser.add_argument('--llvm', action='store_true', help='test optional Clang LLVM emission and round-trip')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    if not shutil.which('cargo'):
        raise SystemExit('Rust/Cargo is required for end-to-end validation.')
    compilers = args.cc or [c for c in ('gcc', 'clang') if shutil.which(c)]
    if not compilers:
        raise SystemExit('GCC/Clang is required')
    command(['python3', 'scripts/generate_v2_tests.py', '--check'])
    command(['python3', 'scripts/generate_v3_tests.py', '--check'])
    command(['python3', 'scripts/generate_v4_tests.py', '--check'])
    command(['python3', 'scripts/generate_v5_tests.py', '--check'])
    cases = [(version, case) for version in (2, 3, 4, 5)
             for case in json.loads((ROOT / f'tests/v{version}-cases.json').read_text())]
    for name, spec in json.loads((ROOT/'tests/v4-examples.json').read_text()).items():
        EXAMPLES[name] = spec['stdout']
    for name, spec in json.loads((ROOT/'tests/v5-examples.json').read_text()).items():
        EXAMPLES[name] = spec['stdout']
    results = []
    rejected = []
    for cc in compilers:
        if not shutil.which(cc):
            raise SystemExit(f'C compiler not found: {cc}')
        env = dict(os.environ, NACRE_CC=cc)
        command(['cargo', 'test', '--offline', '--locked'], env=env)
        command(['cargo', 'build', '--offline', '--locked'], env=env)
        target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
        if not target.is_absolute():
            target = ROOT / target
        nacre = target / 'debug' / ('nacre.exe' if os.name == 'nt' else 'nacre')
        with tempfile.TemporaryDirectory(prefix='nacre-validation-') as temporary:
            directory = Path(temporary)
            for name, stdout in EXAMPLES.items():
                source = example_source(name)
                results.append(exercise_native(nacre, source, name, stdout,
                                               cc, directory, env, args.sanitizers))
            results.append(exercise_native(nacre, ROOT/'examples/language/multi_file_project', 'multifile_project',
                'Sum = 42\n', cc, directory, env, args.sanitizers))
            command([str(nacre), 'doctor'], env=env)
            for version, case in cases:
                name = f'v{version}_' + case['name']
                source = directory / f'{name}.ncr'
                source.write_text(case['source'], encoding='utf-8')
                check = subprocess.run([str(nacre), 'check', str(source)],
                                       capture_output=True, text=True, timeout=20)
                if not case['valid']:
                    assert check.returncode != 0, ('invalid source accepted', name)
                    if case.get('error', '').startswith('E'):
                        assert case.get('error', '') in check.stderr, (name, check.stderr)
                    else:
                        header = check.stderr.partition('\n')[0]
                        location, separator, _ = header.partition(': error:')
                        assert separator and location.startswith(f'{source}:'), (name, check.stderr)
                        coordinates = location[len(str(source)) + 1:].split(':')
                        assert len(coordinates) == 2 and all(part.isdigit() for part in coordinates), (name, check.stderr)
                    rejected.append({'compiler': cc, 'case': name, 'status': 'correctly_rejected'})
                else:
                    assert check.returncode == 0, (name, check.stderr)
                    if 'stdout' in case:
                        results.append(exercise_native(nacre, source, name, case['stdout'],
                            cc, directory, env, args.sanitizers))
                    if 'runtime_error' in case:
                        # Fatal runtime errors terminate without unwinding. Run
                        # these exit-code checks separately from leak checks.
                        c_file = directory / (name + '.c')
                        exe = directory / name
                        command([str(nacre), 'emit-c', str(source), '-o', str(c_file)], capture_output=True)
                        command([cc, '-std=c11', '-O2', '-pthread', str(c_file), '-lm', '-o', str(exe)], capture_output=True)
                        result = subprocess.run([str(exe)], capture_output=True, timeout=20)
                        assert result.returncode == 70, (name, result.returncode, result.stderr)
                        assert b'[1:' in result.stderr, (name, result.stderr)
                        results.append({'compiler': cc, 'case': name, 'status': 'correctly_aborted', 'exit_code': 70, 'sanitizers': False})
    llvm_cases = []
    if args.llvm:
        if not shutil.which(os.environ.get('NACRE_CLANG', 'clang')):
            raise SystemExit('Clang is required for --llvm')
        command(['cargo', 'test', '--offline', '--locked', '--test', 'interfaces_generics_tools', '--', '--ignored'])
        with tempfile.TemporaryDirectory(prefix='nacre-llvm-') as temp:
            directory = Path(temp)
            for name in list(json.loads((ROOT/'tests/v4-examples.json').read_text())) + list(json.loads((ROOT/'tests/v5-examples.json').read_text())):
                source = example_source(name)
                ir = directory/f'{name}.ll'
                exe = directory/name
                command([str(nacre), 'emit-llvm', str(source), '-O2', '--memory-stats', '-o', str(ir)], capture_output=True)
                command([os.environ.get('NACRE_CLANG', 'clang'), str(ir), '-o', str(exe)], capture_output=True)
                result = command([str(exe)], capture_output=True, timeout=20)
                stdout = EXAMPLES[name]
                assert result.stdout == stdout.encode(), (name, result.stdout)
                assert_memory_balanced(result.stderr, name)
                llvm_cases.append({'case':name, 'status':'passed'})
    report = {'scope': 'actual Rust compiler and generated executables', 'llvm_cases':llvm_cases,
              'status': 'passed', 'results': results, 'negative_cases': rejected}
    text = json.dumps(report, indent=2, ensure_ascii=False) + '\n'
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(text, encoding='utf-8')
    print(text)

if __name__ == '__main__':
    try:
        main()
    except (SystemExit, AssertionError, subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as error:
        if isinstance(error, SystemExit) and error.code in (None, 0):
            raise
        report = {'scope': 'actual Rust compiler and generated executables',
                  'status': 'blocked' if not shutil.which('cargo') else 'failed',
                  'reason': str(error), 'rust_compiled': False if not shutil.which('cargo') else 'not_certified'}
        if isinstance(error, subprocess.CalledProcessError):
            report['stderr'] = (error.stderr or b'').decode(errors='replace') if isinstance(error.stderr, bytes) else error.stderr
        if '--report' in sys.argv:
            i = sys.argv.index('--report') + 1
            if i < len(sys.argv):
                path = Path(sys.argv[i]); path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2), file=sys.stderr)
        sys.exit(1)
