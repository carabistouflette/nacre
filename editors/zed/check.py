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
        standard_types = [
            ("Array", "Array<int>"),
            ("Arrays", "Arrays"),
            ("ByteBuffer", "ByteBuffer"),
            ("Channel", "Channel<Text>"),
            ("Command", "Command"),
            ("Comparator", "Comparator<Text>"),
            ("Console", "Console"),
            ("Datagram", "Datagram"),
            ("Deque", "Deque<Text>"),
            ("Directory", "Directory"),
            ("Env", "Env"),
            ("ExitStatus", "ExitStatus"),
            ("File", "File"),
            ("FileInfo", "FileInfo"),
            ("Files", "Files"),
            ("Guard", "Guard<Text>"),
            ("List", "List<long>"),
            ("Map", "Map<Text, long>"),
            ("Math", "Math"),
            ("Mutex", "Mutex<Text>"),
            ("Option", "Option<long>"),
            ("Path", "Path"),
            ("Process", "Process"),
            ("ProcessOutput", "ProcessOutput"),
            ("Random", "Random"),
            ("Receiver", "Receiver<Text>"),
            ("Regex", "Regex"),
            ("RegexMatch", "RegexMatch"),
            ("Result", "Result<Unit, Text>"),
            ("SendError", "SendError<Text>"),
            ("Sender", "Sender<Text>"),
            ("Set", "Set<Text>"),
            ("SocketAddress", "SocketAddress"),
            ("String", "String"),
            ("Task", "Task<long>"),
            ("TcpListener", "TcpListener"),
            ("TcpStream", "TcpStream"),
            ("Text", "Text"),
            ("Thread", "Thread<long>"),
            ("Time", "Time"),
            ("TlsConfig", "TlsConfig"),
            ("TlsStream", "TlsStream"),
            ("UdpSocket", "UdpSocket"),
            ("Uri", "Uri"),
            ("WebSocket", "WebSocket"),
            ("WebSocketMessage", "WebSocketMessage"),
        ]
        type_source = (
            "void standardTypes("
            + ", ".join(f"{type_expression} value{index}" for index, (_, type_expression) in enumerate(standard_types))
            + ") {}"
        ).encode()
        tree = parser.parse(type_source)
        assert not tree.root_node.has_error, tree.root_node
        captures = QueryCursor(queries["highlights"]).captures(tree.root_node)
        builtin_type_text = {
            type_source[node.start_byte:node.end_byte].decode()
            for node in captures.get("type.builtin", [])
        }
        expected_builtin_type_text = {name for name, _ in standard_types}
        assert expected_builtin_type_text.issubset(builtin_type_text), (
            expected_builtin_type_text - builtin_type_text
        )

        alpha_syntax = b'''module samples.editor;
import samples.library.Factory;
import samples.library.*;

public class Factory<T> {
    public static T identity(T value) { return value; }
}

Result<int, Text> parse(borrow Text input) {
    return new Result<int, Text>.Ok(input.parseInt()?);
}

void main() {
    List<List<long>> nested = new List<List<long>>();
    List<long> values = samples.library.Factory<List<long>>.identity(new List<long>());
    byte marker = (byte)65;
    char scalar = '\\u{1f9ea}';
    long count = 42L;
    double precise = (double)(int)3.75;
    Unit unit = ();
    long shifted = count >> 2;
    for (long item : values) {}
    for (long key, borrow Text value : counts) {}
}'''
        tree = parser.parse(alpha_syntax)
        assert not tree.root_node.has_error, tree.root_node
        captures = QueryCursor(queries["highlights"]).captures(tree.root_node)
        captured_text = {
            capture: {alpha_syntax[node.start_byte:node.end_byte].decode() for node in nodes}
            for capture, nodes in captures.items()
        }
        assert {"samples", "library"}.issubset(captured_text.get("namespace", set()))
        assert "Factory" in captured_text.get("type.builtin", set()) or "Factory" in captured_text.get("type", set())
        assert {"identity", "parseInt"}.issubset(captured_text.get("function", set()))
        assert {">>", "?"}.issubset(captured_text.get("operator", set()))
        assert {"byte", "char", "long", "double", "Unit"}.issubset(captured_text.get("type.builtin", set()))
        assert {"42L", "3.75"}.issubset(captured_text.get("number", set()))
        assert "scalar" in captured_text.get("variable", set())
        print("Verified qualified static calls, module paths, numeric literals and casts, result propagation, and generic closers.")

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
        map_traversal = b'''void main() {
    for (int item : numbers) {}
    for (int key, int value : counts) {}
    for (int key, borrow Counter value : counters) {}
    for (int key, borrow mut Counter value : counters) {}
}'''
        tree = parser.parse(map_traversal)
        assert not tree.root_node.has_error, tree.root_node
        captures = QueryCursor(queries["highlights"]).captures(tree.root_node)
        captured_text = {
            capture: [map_traversal[node.start_byte:node.end_byte].decode() for node in nodes]
            for capture, nodes in captures.items()
        }
        assert sorted(captured_text.get("variable.parameter", [])) == sorted([
            "item", "key", "value", "key", "value", "key", "value"
        ]), captured_text.get("variable.parameter")
        assert captured_text.get("type.builtin", []).count("int") == 5
        assert captured_text.get("type", []).count("Counter") == 2
        assert {"for", "borrow", "mut"}.issubset(set(captured_text.get("keyword", [])))
        assert not {"item", "key", "value"} & set(captured_text.get("type", []))
        print("Verified list/map binding captures for key/value types, names, borrow modes, and lexical scope.")
        text_map_traversal = b'''void main() {
    for (borrow Text key, int count : counts) {}
    for (borrow Text key, borrow Counter shared : counters) {}
    for (borrow Text key, borrow mut Counter writable : counters) {}
    // borrow mut Text key
    println("borrow mut Text key");
}'''
        tree = parser.parse(text_map_traversal)
        assert not tree.root_node.has_error, tree.root_node
        binding_query = Query(language, """
            (for_statement
              key_type: (borrowed_type) @key_type
              key: (identifier) @key_name
              value_type: (_) @value_type
              value: (identifier) @value_name)
        """)
        bindings = QueryCursor(binding_query).captures(tree.root_node)
        binding_text = {
            capture: [text_map_traversal[node.start_byte:node.end_byte].decode() for node in nodes]
            for capture, nodes in bindings.items()
        }
        assert binding_text.get("key_type") == ["borrow Text"] * 3, binding_text
        assert binding_text.get("key_name") == ["key"] * 3, binding_text
        assert sorted(binding_text.get("value_type", [])) == sorted(["int", "borrow Counter", "borrow mut Counter"]), binding_text
        assert sorted(binding_text.get("value_name", [])) == ["count", "shared", "writable"], binding_text

        captures = QueryCursor(queries["highlights"]).captures(tree.root_node)
        captured_text = {
            capture: [text_map_traversal[node.start_byte:node.end_byte].decode() for node in nodes]
            for capture, nodes in captures.items()
        }
        assert sorted(captured_text.get("variable.parameter", [])) == sorted([
            "key", "count", "key", "shared", "key", "writable"
        ]), captured_text.get("variable.parameter")
        assert captured_text.get("type", []).count("Text") == 3
        assert captured_text.get("type", []).count("Counter") == 2
        assert captured_text.get("type.builtin", []).count("int") == 1
        assert {"borrow", "mut"}.issubset(set(captured_text.get("keyword", [])))
        assert not {"key", "count", "shared", "writable"} & set(captured_text.get("type", []))
        for capture in ["comment", "string"]:
            contents = [
                (node.start_byte, node.end_byte)
                for node in captures.get(capture, [])
            ]
            for node in captures.get("keyword", []):
                assert not any(start <= node.start_byte < end for start, end in contents), (
                    f"keyword {text_map_traversal[node.start_byte:node.end_byte]!r} leaked into {capture}"
                )
        print("Verified shared Text key/copy and borrowed value modes, query captures, and comment/string scope isolation.")
        print(f"Zed smoke: {count} examples parsed without errors; all {len(queries)} queries executed.")
        print("Verified ownership keywords, enum/match syntax, variant construction, consuming bindings, string/comment isolation, escapes, types, properties, and Sudoku outline.")


if __name__ == "__main__":
    main()
