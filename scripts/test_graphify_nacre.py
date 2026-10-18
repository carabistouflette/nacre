#!/usr/bin/env python3
"""Regression tests for the Nacre Graphify source adapter."""
from pathlib import Path
import tempfile
import unittest

from graphify_nacre import _PREFIX, enrich_graph


class GraphifyNacreTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="nacre-graphify-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.sources = {
            "stdlib/prelude/text.ncr": "class Text {\n    public static Text join(borrow List<Text> parts, borrow Text delimiter) {}\n}\n",
            "src/stdlib.rs": "    (C::Text, \"join\") => M::TextJoin,\n",
            "src/ir.rs": "    TextSplit, TextJoin,\n",
            "src/stdlib_codegen/text.rs": (
                "    let join = native_function(program, |method| matches!(method, M::TextJoin));\n"
                "    let template = (\"@TEXT_JOIN@\", format!(\"ncr_fn_{}\", join.id));\n"
            ),
            "src/stdlib_codegen/support.rs": "    M::TextJoin => \"@TEXT_JOIN@\",\n",
            "stdlib/text.c.in": "static ncr_obj_@ID@ *@TEXT_JOIN@(parts, delimiter) {\n}\n",
        }
        for relative_path, content in self.sources.items():
            path = self.root / relative_path
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        self.graph = {
            "built_at_commit": "fixture",
            "nodes": [
                {"id": "textjoin-ir", "label": "TextJoin", "source_file": "src/ir.rs"},
                {"id": "unrelated", "label": "Unrelated", "source_file": "src/other.rs"},
            ],
            "links": [
                {"source": "textjoin-ir", "target": "unrelated", "relation": "fixture"},
            ],
        }

    def test_emits_complete_source_path_and_preserves_existing_graph(self):
        updated = enrich_graph(self.graph, self.root)
        nodes = {node["id"]: node for node in updated["nodes"]}
        self.assertIn("unrelated", nodes)
        self.assertEqual(nodes["nacre_graphify_text_join_api"]["source_file"], "stdlib/prelude/text.ncr")
        self.assertEqual(nodes["nacre_graphify_text_join_api"]["source_location"], "L2")
        self.assertEqual(nodes["nacre_graphify_text_join_implementation"]["source_file"], "stdlib/text.c.in")

        path = [
            "nacre_graphify_text_join_api",
            "nacre_graphify_text_join_registration",
            "textjoin-ir",
            "nacre_graphify_text_join_selector",
            "nacre_graphify_text_join_dispatch",
            "nacre_graphify_text_join_binding",
            "nacre_graphify_text_join_implementation",
        ]
        edges = {(edge["source"], edge["target"]): edge for edge in updated["links"]}
        self.assertTrue(all((source, target) in edges for source, target in zip(path, path[1:])))
        self.assertEqual(edges[(path[0], path[1])]["relation"], "registered_by")
        self.assertEqual(edges[(path[-2], path[-1])]["relation"], "implemented_by")
        self.assertEqual(edges[(path[-2], path[-1])]["confidence"], "EXTRACTED")
        self.assertEqual(updated["built_at_commit"], "fixture")

    def test_repeated_enrichment_is_idempotent(self):
        first = enrich_graph(self.graph, self.root)
        second = enrich_graph(first, self.root)
        generated = [node for node in second["nodes"] if node["id"].startswith(_PREFIX)]
        self.assertEqual(len(generated), 6)
        self.assertEqual(len(second["links"]), 7)

    def test_ambiguous_source_anchor_fails_without_mutating_input(self):
        api_path = self.root / "stdlib/prelude/text.ncr"
        api_path.write_text(api_path.read_text(encoding="utf-8") * 2, encoding="utf-8")
        original_nodes = list(self.graph["nodes"])
        with self.assertRaisesRegex(ValueError, "found 2"):
            enrich_graph(self.graph, self.root)
        self.assertEqual(self.graph["nodes"], original_nodes)

    def test_missing_ir_node_fails_closed(self):
        graph = dict(self.graph)
        graph["nodes"] = [node for node in graph["nodes"] if node["label"] != "TextJoin"]
        with self.assertRaisesRegex(ValueError, "expected one Graphify TextJoin node"):
            enrich_graph(graph, self.root)


if __name__ == "__main__":
    unittest.main()
