#!/usr/bin/env python3
"""Limited brace check of simple Rust formatting literals; NOT a Rust compiler.

The regular expression deliberately does not cover every Rust macro or literal.
A successful result is not a syntax, type, ownership, or execution validation.
"""
from pathlib import Path
import json
import re

ROOT = Path(__file__).resolve().parents[1]
PATTERN = re.compile(r'(?:format|println|eprintln|write|writeln|panic)!\(\s*(?:[^"\n]*,\s*)?("(?:\\.|[^"\\])*")')


def main():
    count = 0
    for path in sorted((ROOT/'src').glob('*.rs')):
        source = path.read_text()
        for match in PATTERN.finditer(source):
            text = match[1][1:-1]
            i = 0
            count += 1
            while i < len(text):
                if text[i] not in '{}':
                    i += 1
                    continue
                if text[i:i+2] in ('{{', '}}'):
                    i += 2
                    continue
                line = source[:match.start()].count('\n') + 1
                if text[i] == '}':
                    raise SystemExit(f'{path}:{line}: unmatched closing format brace')
                end = text.find('}', i + 1)
                if end < 0 or '{' in text[i+1:end]:
                    raise SystemExit(f'{path}:{line}: malformed format brace')
                i = end + 1
    print(json.dumps({'scope': 'limited brace scan of simple Rust format literals ONLY',
                      'rust_compiled': False, 'literals_checked': count,
                      'status': 'passed'}, indent=2))


if __name__ == '__main__':
    main()
