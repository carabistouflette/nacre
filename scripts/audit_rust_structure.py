#!/usr/bin/env python3
"""Delimiter-only structural audit. NOT a Rust parser, type checker, or test run.

Skips strings, char literals, lifetimes, and nested comments before balancing
(), [] and {}. Intended to catch accidental truncation while packaging.
"""
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
CHAR = re.compile(r"'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])'")
RAW = re.compile(r'r(\#*)"')

def audit(path):
    text = path.read_text(encoding='utf-8')
    stack = []
    i = 0
    count = 0
    while i < len(text):
        if text.startswith('//', i):
            end = text.find('\n', i)
            i = len(text) if end < 0 else end + 1
            continue
        if text.startswith('/*', i):
            depth = 1
            i += 2
            while depth and i < len(text):
                if text.startswith('/*', i):
                    depth += 1; i += 2
                elif text.startswith('*/', i):
                    depth -= 1; i += 2
                else:
                    i += 1
            assert depth == 0, (path, 'unterminated comment')
            continue
        raw = RAW.match(text, i)
        if raw:
            terminator = '"' + raw[1]
            end = text.find(terminator, raw.end())
            assert end >= 0, (path, 'unterminated raw string')
            i = end + len(terminator)
            continue
        if text[i] == '"':
            i += 1
            while i < len(text):
                if text[i] == '\\':
                    i += 2
                elif text[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            else:
                raise AssertionError((path, 'unterminated string'))
            continue
        char = CHAR.match(text, i)
        if char:
            i = char.end()
            continue
        c = text[i]
        if c in '([{':
            stack.append((c, i)); count += 1
        elif c in ')]}':
            assert stack, (path, 'unexpected closer', c, i)
            opener, at = stack.pop()
            assert opener == {')': '(', ']': '[', '}': '{'}[c], (path, 'mismatch', at, i)
        i += 1
    assert not stack, (path, 'unclosed delimiters', stack[-3:])
    return {'file': str(path.relative_to(ROOT)), 'balanced_pairs': count}

def main():
    files = sorted((ROOT/'src').glob('*.rs')) + sorted((ROOT/'tests').glob('*.rs'))
    results = [audit(path) for path in files]
    print(json.dumps({'scope': 'delimiter balance only; NOT Rust syntax or type validation',
                      'rust_tests_executed': False, 'status': 'passed', 'results': results}, indent=2))

if __name__ == '__main__':
    main()
