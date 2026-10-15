#!/usr/bin/env python3
"""Profile release Nacre builds; tool durations are real subprocess wall times.

The instrumented pass uses executable wrappers (their startup cost remains in
build wall time). Uninstrumented timings are separate. Clang's backend/driver
measurement subtracts its nested native linker, not LLVM optimization alone.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def run(command, env):
    start = time.perf_counter_ns()
    result = subprocess.run([str(x) for x in command], env=env, capture_output=True, timeout=120)
    elapsed = (time.perf_counter_ns() - start) / 1e9
    if result.returncode:
        raise RuntimeError(f"{command}: {result.stderr.decode(errors='replace')}")
    return elapsed, result


def tool():
    role, binary, logdir, ld_wrapper, *arguments = sys.argv[2:]
    query = '--version' in arguments or '-print-target-triple' in arguments
    linking = not query and '-c' not in arguments and '-S' not in arguments
    if role == 'clang' and linking:
        arguments.append(f'-fuse-ld={ld_wrapper}')
    start = time.perf_counter_ns()
    result = subprocess.run([binary, *arguments])
    elapsed = (time.perf_counter_ns() - start) / 1e9
    stage = 'tool_probe' if query else {
        'ld': 'native_link', 'llvm-link': 'llvm_link',
        'clang': 'clang_native' if linking else 'runtime_bitcode',
    }[role]
    Path(logdir, f'{os.getpid()}.json').write_text(json.dumps({
        'stage': stage, 'seconds': elapsed, 'arguments': arguments,
        'returncode': result.returncode,
    }))
    raise SystemExit(result.returncode)


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def fixtures(directory):
    small = directory / 'counter.ncr'
    collection = directory / 'inventory.ncr'
    app = directory / 'app'
    for name, dependencies, module, source in [
        ('app', '\n[dependencies]\nlib_a = { path = "../lib-a" }\n', 'app',
         'import deps.a.value;\nvoid main() { println(value()); }\n'),
        ('lib-a', '\n[dependencies]\nlib_b = { path = "../lib-b" }\n', 'deps.a',
         'import deps.b.answer;\npublic int value() { return answer(); }\n'),
        ('lib-b', '', 'deps.b', 'public int answer() { return 42; }\n'),
    ]:
        write(directory / name / 'Nacre.toml',
              f'[package]\nname = "{name.replace("-", "_")}"\nversion = "0.1.0"\nsources = ["src"]\n' + dependencies)
        write(directory / name / 'src/Main.ncr', f'module {module};\n{source}')
    def update(nonce):
        small.write_text((ROOT / 'examples/language/counter.ncr').read_text().replace('new Counter(40)', f'new Counter({40 + nonce})'))
        collection.write_text((ROOT / 'examples/collections/text_map_inventory.ncr').read_text().replace('"Coffee beans"), 12)', f'"Coffee beans"), {12 + nonce})'))
        (directory / 'lib-b/src/Main.ncr').write_text(f'module deps.b;\npublic int answer() {{ return {42 + nonce}; }}\n')
        return {'counter': f'{42 + nonce}\n',
                'inventory': f'true\ntrue\n{12 + nonce}\n{20 + nonce}\n2\n',
                'packages': f'{42 + nonce}\n'}
    return {'counter': small, 'inventory': collection, 'packages': app}, update


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--label', required=True)
    parser.add_argument('--repeats', type=int, default=3)
    args = parser.parse_args()
    if not 1 <= args.repeats <= 20:
        parser.error('--repeats must be 1..20')
    compiler = args.compiler.resolve()
    env = os.environ.copy()
    env.pop('NACRE_NO_CACHE', None)
    env['NACRE_ENABLE_TEST_CACHE'] = '1'
    clang = shutil.which(env.get('NACRE_CLANG', 'clang'))
    llvm_link = shutil.which(env.get('NACRE_LLVM_LINK', 'llvm-link'))
    ld = shutil.which('ld')
    if not all((clang, llvm_link, ld)):
        parser.error('Clang, llvm-link and ld must be available')
    env['NACRE_CC'] = env['NACRE_CLANG'] = clang
    cpu = min(os.sched_getaffinity(0))
    os.sched_setaffinity(0, {cpu})
    rows = []
    with tempfile.TemporaryDirectory(prefix='nacre-profile-') as temp:
        directory = Path(temp)
        inputs, update = fixtures(directory)
        wrappers = {}
        logs = directory / 'logs'
        logs.mkdir()
        for role, binary in [('ld', ld), ('clang', clang), ('llvm-link', llvm_link)]:
            path = directory / role
            path.write_text('#!' + sys.executable + '\nimport os,sys\nos.execv(' + repr(sys.executable) + ', ' + repr([sys.executable, str(Path(__file__).resolve()), '--tool', role, binary, str(logs), str(directory / 'ld')]) + ' + sys.argv[1:])\n')
            path.chmod(0o700)
            wrappers[role] = str(path)
        nonce = 0
        for instrumented in (False, True):
            build_env = env.copy()
            if instrumented:
                build_env.update(NACRE_CC=wrappers['clang'], NACRE_CLANG=wrappers['clang'], NACRE_LLVM_LINK=wrappers['llvm-link'])
            for name, source in inputs.items():
                for backend in ('c', 'llvm'):
                    cache = directory / f'cache-{instrumented}-{name}-{backend}'
                    build_env['NACRE_CACHE_DIR'] = str(cache)
                    for sample in range(args.repeats):
                        for state in ('cold', 'warm_changed_source'):
                            if state == 'cold':
                                shutil.rmtree(cache, ignore_errors=True)
                            nonce += 1
                            expected = update(nonce)[name].encode()
                            for old in logs.iterdir():
                                old.unlink()
                            output = directory / 'program'
                            command = [compiler, 'build', source, '-O2', '--memory-stats', '-o', output]
                            if backend == 'llvm':
                                command.append('--direct')
                            elapsed, _ = run(command, build_env)
                            _, result = run([output], build_env)
                            if result.stdout != expected or b'live=0' not in result.stderr:
                                raise RuntimeError(f'{name}/{backend}: stdout={result.stdout!r}, stderr={result.stderr!r}, expected={expected!r}')
                            events = [json.loads(p.read_text()) for p in sorted(logs.iterdir())]
                            # emit-* isolates source loading/checking/codegen, not cache/tool work.
                            emit = [compiler, 'emit-c' if backend == 'c' else 'emit-llvm', source, '-o', directory / 'emitted']
                            if backend == 'llvm':
                                emit.append('--direct')
                            frontend, _ = run(emit, env)
                            row = dict(consumer=name, backend=backend, cache=state,
                                       instrumented=instrumented, sample=sample,
                                       build_seconds=elapsed, frontend_codegen_seconds=frontend,
                                       stdout=result.stdout.decode(), memory=result.stderr.decode(), events=events)
                            rows.append(row)
                            print(f'{args.label} {name} {backend} {state} traced={instrumented}: {elapsed:.4f}s', file=sys.stderr)
    data = dict(label=args.label, platform=platform.platform(), cpu_affinity=cpu,
                compiler_sha256=hashlib.sha256(compiler.read_bytes()).hexdigest(),
                compiler=str(compiler), repeats=args.repeats,
                clang=run([clang, '--version'], env)[1].stdout.decode(),
                llvm_link=run([llvm_link, '--version'], env)[1].stdout.decode(),
                methodology='Sequential pinned release compiler; Clang -O2 both backends; new executable source literal each build; cold isolated cache; warm runtime cache; wrappers add startup overhead; native_link nested inside clang_native.', rows=rows)
    args.output.write_text(json.dumps(data, indent=2) + '\n')


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--tool':
        tool()
    else:
        main()
