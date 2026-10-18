#!/usr/bin/env python3
"""Add deterministic Text.join links for Nacre files Graphify cannot parse.

Run after ``graphify update .`` and before ``graphify cluster-only .``. Graphify's
stock detector does not classify ``.ncr`` or ``.c.in`` as code; this adapter
anchors the public API, native registration, IR variant, codegen marker, and C
implementation without modifying the installed Graphify package.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import tempfile
from pathlib import Path
from typing import Any


_PREFIX = "nacre_graphify_text_join_"
_ANCHORS = {
    "api": (
        "Text.join API declaration",
        "stdlib/prelude/text.ncr",
        re.compile(
            r"\bpublic\s+static\s+Text\s+join\s*\(\s*"
            r"borrow\s+List\s*<\s*Text\s*>\s+parts\s*,\s*"
            r"borrow\s+Text\s+delimiter\s*\)"
        ),
    ),
    "registration": (
        "Text.join native registration",
        "src/stdlib.rs",
        re.compile(r'\(C::Text,\s*"join"\)\s*=>\s*M::TextJoin\b'),
    ),
    "selector": (
        "TextJoin codegen selector",
        "src/stdlib_codegen/text.rs",
        re.compile(
            r"\blet\s+join\s*=\s*native_function\(program,\s*"
            r"\|method\|\s*matches!\(method,\s*M::TextJoin\)"
        ),
    ),
    "dispatch": (
        "TextJoin C placeholder dispatch",
        "src/stdlib_codegen/support.rs",
        re.compile(r'\bM::TextJoin\s*=>\s*"@TEXT_JOIN@"'),
    ),
    "binding": (
        "TextJoin C symbol binding",
        "src/stdlib_codegen/text.rs",
        re.compile(r'\("@TEXT_JOIN@",\s*format!\("ncr_fn_\{\}",\s*join\.id\)\)'),
    ),
    "implementation": (
        "Text.join C template implementation",
        "stdlib/text.c.in",
        re.compile(r"@TEXT_JOIN@\s*\("),
    ),
}

_RELATIONS = (
    ("api", "registration", "registered_by", "registration"),
    ("registration", "ir", "selects", "registration"),
    ("ir", "selector", "selected_by", "selector"),
    ("selector", "dispatch", "dispatches_to", "dispatch"),
    ("dispatch", "binding", "bound_by", "binding"),
    ("binding", "implementation", "implemented_by", "implementation"),
)


def _find_line(root: Path, relative_path: str, pattern: re.Pattern[str], description: str) -> int:
    path = root / relative_path
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise ValueError(f"cannot read {relative_path}: {error}") from error
    matches = [index + 1 for index, line in enumerate(lines) if pattern.search(line)]
    if len(matches) != 1:
        raise ValueError(
            f"expected exactly one {description} in {relative_path}, found {len(matches)}"
        )
    return matches[0]


def _node(node_id: str, label: str, source_file: str, line: int) -> dict[str, Any]:
    return {
        "id": node_id,
        "label": label,
        "_origin": "custom",
        "file_type": "code",
        "source_file": source_file,
        "source_location": f"L{line}",
    }


def _edge(source: str, target: str, relation: str, source_file: str, line: int) -> dict[str, Any]:
    return {
        "source": source,
        "target": target,
        "relation": relation,
        "_origin": "custom",
        "confidence": "EXTRACTED",
        "confidence_score": 1.0,
        "source_file": source_file,
        "source_location": f"L{line}",
        "weight": 1.0,
    }


def enrich_graph(graph: dict[str, Any], root: Path) -> dict[str, Any]:
    """Return a graph with an idempotent, source-anchored Text.join path."""
    if not isinstance(graph.get("nodes"), list) or not isinstance(graph.get("links"), list):
        raise ValueError("Graphify JSON must contain node and link arrays")

    ir_nodes = [
        node for node in graph["nodes"]
        if node.get("label") == "TextJoin" and node.get("source_file") == "src/ir.rs"
    ]
    if len(ir_nodes) != 1:
        raise ValueError(f"expected one Graphify TextJoin node in src/ir.rs, found {len(ir_nodes)}")
    ir_id = ir_nodes[0].get("id")
    if not isinstance(ir_id, str):
        raise ValueError("Graphify TextJoin node has no string id")

    root = root.resolve()
    lines = {
        key: _find_line(root, path, pattern, description)
        for key, (description, path, pattern) in _ANCHORS.items()
    }
    node_ids = {key: f"{_PREFIX}{key}" for key in _ANCHORS}
    labels = {key: description for key, (description, _, _) in _ANCHORS.items()}
    nodes = [
        _node(node_ids[key], labels[key], _ANCHORS[key][1], lines[key])
        for key in _ANCHORS
    ]

    ir_key = "ir"
    all_ids = {node.get("id") for node in graph["nodes"]}
    if ir_id in node_ids.values():
        raise ValueError("Graphify TextJoin id collides with a supplemental node")
    all_ids.update(node_ids.values())
    edges = []
    for source_key, target_key, relation, evidence_key in _RELATIONS:
        source_id = ir_id if source_key == ir_key else node_ids[source_key]
        target_id = ir_id if target_key == ir_key else node_ids[target_key]
        _, source_file, _ = _ANCHORS[evidence_key]
        edges.append(_edge(source_id, target_id, relation, source_file, lines[evidence_key]))

    replacement_ids = set(node_ids.values())
    result = dict(graph)
    result["nodes"] = [
        node for node in graph["nodes"] if node.get("id") not in replacement_ids
    ] + nodes
    result["links"] = [
        edge for edge in graph["links"]
        if edge.get("source") not in replacement_ids and edge.get("target") not in replacement_ids
    ] + edges

    if any(edge.get("source") not in all_ids or edge.get("target") not in all_ids for edge in edges):
        raise ValueError("supplemental Text.join edge has a missing endpoint")
    return result


def _write_graph(path: Path, graph: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary_path: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=path.parent,
            prefix=f".{path.name}.", suffix=".tmp", delete=False,
        ) as temporary:
            temporary_path = Path(temporary.name)
            json.dump(graph, temporary, ensure_ascii=False, indent=2)
            temporary.write("\n")
        os.replace(temporary_path, path)
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parents[1],
        help="Nacre project root (default: script's parent directory)",
    )
    parser.add_argument(
        "--graph", type=Path,
        help="Graphify JSON path, relative to --root if not absolute (default: graphify-out/graph.json)",
    )
    args = parser.parse_args()
    root = args.root.resolve()
    graph_path = args.graph or Path("graphify-out/graph.json")
    if not graph_path.is_absolute():
        graph_path = root / graph_path
    try:
        graph = json.loads(graph_path.read_text(encoding="utf-8"))
        updated = enrich_graph(graph, root)
        _write_graph(graph_path, updated)
    except (OSError, json.JSONDecodeError, ValueError) as error:
        parser.error(str(error))
    print(
        f"Added {len(_ANCHORS)} Nacre source nodes and {len(_RELATIONS)} links; "
        f"graph now has {len(updated['nodes'])} nodes and {len(updated['links'])} links."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
