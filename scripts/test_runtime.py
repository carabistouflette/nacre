#!/usr/bin/env python3
"""Independent C-runtime checks, not a substitute for cargo test.

Uses only the Python standard library and an installed GCC/Clang toolchain.
The exact runtime.c included by the Rust backend is tested with sanitizers.
"""
import argparse
import ctypes
import json
import random
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIN = -(1 << 31)
MAX = (1 << 31) - 1


def wrap(value):
    return ((value + (1 << 31)) % (1 << 32)) - (1 << 31)


def quotient(a, b):
    q = abs(a) // abs(b)
    return -q if (a < 0) != (b < 0) else q


def check(compiler):
    runtime = (ROOT / "src/runtime.c").read_text()
    with tempfile.TemporaryDirectory(prefix="nacre-runtime-check-") as d:
        directory = Path(d)
        wrapper = runtime + "\n" + "\n".join([
            "int32_t test_add(int32_t a, int32_t b) { return ncr_add(a,b); }",
            "int32_t test_sub(int32_t a, int32_t b) { return ncr_sub(a,b); }",
            "int32_t test_mul(int32_t a, int32_t b) { return ncr_mul(a,b); }",
            "int32_t test_div(int32_t a, int32_t b) { return ncr_div(a,b,1,1); }",
            "int32_t test_rem(int32_t a, int32_t b) { return ncr_rem(a,b,1,1); }",
            "int32_t test_neg(int32_t a) { return ncr_neg(a); }",
        ])
        c = directory / "test.c"
        so = directory / "test.so"
        c.write_text(wrapper)
        subprocess.run([compiler, "-std=c11", "-O2", "-shared", "-fPIC", str(c), "-o", str(so)], check=True, capture_output=True)
        lib = ctypes.CDLL(str(so))
        ops = {"add": lambda a, b: wrap(a+b), "sub": lambda a, b: wrap(a-b),
               "mul": lambda a, b: wrap(a*b), "div": lambda a, b: wrap(quotient(a,b)),
               "rem": lambda a, b: a - quotient(a,b)*b}
        for name in ops:
            f = getattr(lib, "test_"+name)
            f.argtypes = [ctypes.c_int32, ctypes.c_int32]
            f.restype = ctypes.c_int32
        lib.test_neg.argtypes = [ctypes.c_int32]
        lib.test_neg.restype = ctypes.c_int32
        edge = [MIN, MIN+1, -1_000_000, -3, -1, 0, 1, 3, 1_000_000, MAX-1, MAX]
        rng = random.Random(20261011)
        pairs = [(a, b) for a in edge for b in edge]
        pairs += [(rng.randint(MIN, MAX), rng.randint(MIN, MAX)) for _ in range(10000)]
        assertions = 0
        for a, b in pairs:
            for name, expected in ops.items():
                if name in ("div", "rem") and b == 0:
                    continue
                actual = getattr(lib, "test_"+name)(a, b)
                assert actual == expected(a, b), (compiler, name, a, b, actual, expected(a,b))
                assertions += 1
            assert lib.test_neg(a) == wrap(-a)
            assertions += 1
        # Undefined-behavior sanitizer, arithmetic boundaries, and static strings.
        body = r'''
static const unsigned char text[] = {97,0,98,0};
int main(void) {
    ncr_print_int(ncr_add(INT32_MAX, 1));
    ncr_print_int(ncr_sub(INT32_MIN, 1));
    ncr_print_int(ncr_mul(INT32_MAX, INT32_MAX));
    ncr_print_int(ncr_mul(INT32_MIN, INT32_MIN));
    ncr_print_int(ncr_div(INT32_MIN, -1, 1, 1));
    ncr_print_int(ncr_rem(INT32_MIN, -1, 1, 1));
    ncr_print_int(ncr_neg(INT32_MIN));
    ncr_print_boolean(true);
    ncr_string s = {text, 3};
    ncr_print_string(s);
    return 0;
}
'''
        c.write_text(runtime + body)
        executable = directory / "sanitized"
        subprocess.run([compiler, "-std=c11", "-O2", "-fsanitize=undefined", "-fno-sanitize-recover=all", str(c), "-o", str(executable)], check=True, capture_output=True)
        out = subprocess.run([str(executable)], check=True, capture_output=True)
        assert out.stdout == b"-2147483648\n2147483647\n1\n0\n-2147483648\n0\n-2147483648\ntrue\na\0b\n", out.stdout
        assert not out.stderr, out.stderr
        for operation in ("div", "rem"):
            c.write_text(runtime + f"\nint main(void) {{ return ncr_{operation}(1,0,2,3); }}\n")
            subprocess.run([compiler, "-std=c11", "-O2", str(c), "-o", str(executable)], check=True, capture_output=True)
            out = subprocess.run([str(executable)], capture_output=True)
            assert out.returncode == 70, out
            assert b"[2:3]" in out.stderr, out.stderr
        version = subprocess.check_output([compiler, "--version"], text=True).splitlines()[0]
        return {"compiler": version, "arithmetic_assertions": assertions, "ubsan_boundary_check": "passed", "division_and_modulo_zero": "passed", "embedded_nul_string": "passed"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cc", action="append", default=[])
    args = parser.parse_args()
    compilers = args.cc or [name for name in ("gcc", "clang") if shutil.which(name)]
    if not compilers:
        raise SystemExit("GCC/Clang is required")
    results = [check(cc) for cc in compilers]
    print(json.dumps({"scope": "C runtime only; not the Rust compiler", "results": results}, indent=2))


if __name__ == "__main__":
    main()
