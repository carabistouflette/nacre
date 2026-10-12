#!/usr/bin/env python3
"""Smoke the exact parser and queries used by Zed (requires tree-sitter==0.25.2)."""
import ctypes
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
from tree_sitter import Language, Parser, Query, QueryCursor

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
GRAMMAR = HERE.parent / "tree-sitter-nacre"


def main():
    config = tomllib.loads((HERE / "languages/nacre/config.toml").read_text())
    assert config["grammar"] == "nacre" and config["path_suffixes"] == ["ncr"]
    with tempfile.TemporaryDirectory(prefix="nacre-zed-check-") as temporary:
        library_path = Path(temporary) / "nacre.so"
        subprocess.run([
            os.environ.get("NACRE_CC", os.environ.get("CC", "cc")), "-shared", "-fPIC", "-O2",
            "-I", str(GRAMMAR / "src"), str(GRAMMAR / "src/parser.c"), "-o", str(library_path),
        ], check=True)
        library = ctypes.CDLL(str(library_path))
        library.tree_sitter_nacre.restype = ctypes.c_void_p
        capsule_new = ctypes.pythonapi.PyCapsule_New
        capsule_new.restype = ctypes.py_object
        capsule_new.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_void_p]
        language = Language(capsule_new(library.tree_sitter_nacre(), b"tree_sitter.Language", None))
        parser = Parser(language)
        queries = {
            path.stem: Query(language, path.read_text())
            for path in (HERE / "languages/nacre").glob("*.scm")
        }
        all_captures = {}
        count = 0
        sudoku_outline = []
        for path in sorted((ROOT / "examples").rglob("*.ncr")):
            if "invalid" in path.relative_to(ROOT / "examples").parts:
                continue
            source = path.read_bytes()
            tree = parser.parse(source)
            assert not tree.root_node.has_error, f"{path}: {tree.root_node}"
            for name, query in queries.items():
                captures = QueryCursor(query).captures(tree.root_node)
                if name == "highlights":
                    for capture, nodes in captures.items():
                        all_captures.setdefault(capture, set()).update(
                            source[node.start_byte:node.end_byte].decode() for node in nodes
                        )
                if name == "outline" and path.name == "sudoku.ncr":
                    sudoku_outline = [source[n.start_byte:n.end_byte].decode() for n in captures.get("name", [])]
            count += 1
        for keyword in ["borrow", "mut", "readonly", "class", "interface", "implements"]:
            assert keyword in all_captures.get("keyword", set()), f"Missing keyword highlight: {keyword}"
        assert {"Sudoku", "solve", "show", "main"}.issubset(sudoku_outline), sudoku_outline
        assert "canPlace" in all_captures.get("function", set())
        assert "cells" in all_captures.get("property", set())
        assert "int" in all_captures.get("type.builtin", set())
        assert all_captures.get("comment") and all_captures.get("string")
        focused = b'void main() { Text t = new Text("borrow \\n readonly"); /* mut */ println(true); }'
        tree = parser.parse(focused)
        assert not tree.root_node.has_error
        captures = QueryCursor(queries["highlights"]).captures(tree.root_node)
        keyword_text = {focused[n.start_byte:n.end_byte] for n in captures.get("keyword", [])}
        assert not {b"borrow", b"mut", b"readonly"} & keyword_text
        assert captures.get("string.escape") and captures.get("boolean")
        enum_match = b'''enum Choice<T> {
    Empty;
    Item(T);
}
void consume(borrow mut Text input) {}
void main() {
    Choice<Text> choice = new Choice<Text>.Item(new Text("borrow mut readonly enum match"));
    match (choice) {
        Item(value) {
            // borrow mut readonly enum match Item(value)
            consume(value);
        }
        Empty {}
    }
}'''
        tree = parser.parse(enum_match)
        assert not tree.root_node.has_error, tree.root_node
        captures = QueryCursor(queries["highlights"]).captures(tree.root_node)
        captured_text = {
            capture: {enum_match[node.start_byte:node.end_byte].decode() for node in nodes}
            for capture, nodes in captures.items()
        }
        assert {"Choice"}.issubset(captured_text.get("type", set()))
        assert {"enum", "match", "borrow", "mut"}.issubset(captured_text.get("keyword", set()))
        assert {"Empty", "Item"}.issubset(captured_text.get("constructor", set()))
        assert "value" in captured_text.get("variable.parameter", set())
        assert captures.get("comment") and captures.get("string")
        for capture in ["comment", "string"]:
            contents = [
                (node.start_byte, node.end_byte)
                for node in captures[capture]
            ]
            for node in captures.get("keyword", []):
                assert not any(start <= node.start_byte < end for start, end in contents), (
                    f"keyword {enum_match[node.start_byte:node.end_byte]!r} leaked into {capture}"
                )
        outline = QueryCursor(queries["outline"]).captures(tree.root_node)
        outline_names = {enum_match[node.start_byte:node.end_byte].decode() for node in outline.get("name", [])}
        assert {"Choice", "Empty", "Item"}.issubset(outline_names), outline_names
        print(f"Zed smoke: {count} examples parsed without errors; all {len(queries)} queries executed.")
        print("Verified ownership keywords, enum/match syntax, variant construction, consuming bindings, string/comment isolation, escapes, types, properties, and Sudoku outline.")


if __name__ == "__main__":
    main()
