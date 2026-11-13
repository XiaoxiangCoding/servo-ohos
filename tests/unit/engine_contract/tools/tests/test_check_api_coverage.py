import contextlib
import io
import json
import os
import sys
import tempfile
import textwrap
import unittest


sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import check_api_coverage


MANIFEST = """
[manifest]
version = "test"

[[api]]
name = "Scoped"
arity = 1
source = "jsapi"
status = "implemented"
signature = "() -> ()"

[[api]]
name = "Outside"
arity = 1
source = "jsapi"
status = "implemented"
signature = "() -> ()"

[[api]]
name = "Missing"
arity = 1
source = "jsapi"
status = "implemented"
signature = "() -> ()"

[[api]]
name = "Empty"
arity = 1
source = "jsapi"
status = "empty"
signature = "() -> ()"
reason = "silent no-op"
"""


class CoverageTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.manifest = self.write("api-manifest.toml", MANIFEST)

    def write(self, name, content):
        path = os.path.join(self.temporary.name, name)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(textwrap.dedent(content))
        return path

    def scope(self, entries):
        body = ["[scope]\n", f"entries = {len(entries)}\n"]
        for name, status in entries:
            body.append("\n[[entry]]\n")
            body.append(f'name = "{name}"\n')
            body.append(f'status = "{status}"\n')
            body.append('mode = "direct"\n')
            body.append('provenance = ["components/script/test.rs:1"]\n')
        return self.write("api-scope.toml", "".join(body))

    def coverage(self, cases):
        lines = [
            json.dumps({"id": case, "apis": apis})
            for case, apis in cases
        ]
        return self.write("coverage.jsonl", "\n".join(lines) + "\n")

    def run_check(self, coverage, scope):
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            result = check_api_coverage.main(
                ["check_api_coverage.py", coverage, self.manifest, scope]
            )
        return result, output.getvalue()

    def test_requires_every_implemented_scoped_entry(self):
        result, output = self.run_check(
            self.coverage([("case", ["Scoped"])]),
            self.scope([("Scoped", "implemented"), ("Missing", "implemented")]),
        )
        self.assertEqual(result, 1)
        self.assertIn("1 scoped mozjs entry points have no contract case", output)
        self.assertIn("Missing", output)

    def test_ignores_out_of_scope_implemented_entries(self):
        result, output = self.run_check(
            self.coverage([("case", ["Scoped", "Outside"])]),
            self.scope([("Scoped", "implemented")]),
        )
        self.assertEqual(result, 0, output)
        self.assertIn("1/1 scoped implemented entries", output)
        self.assertIn("1 out-of-scope entries exercised and ignored", output)

    def test_requires_every_case_to_exercise_a_scoped_entry(self):
        result, output = self.run_check(
            self.coverage([("case", ["Scoped"]), ("outside-only", ["Outside"])]),
            self.scope([("Scoped", "implemented")]),
        )
        self.assertEqual(result, 1)
        self.assertIn("outside-only: case has no entry in api-scope.toml", output)

    def test_accepts_empty_scope_entry_as_unimplemented(self):
        result, output = self.run_check(
            self.coverage([("case", ["Empty"])]),
            self.scope([("Empty", "empty")]),
        )
        self.assertEqual(result, 0, output)
        self.assertIn("unimplemented coverage ok: 1/1", output)

    def test_requires_a_case_for_an_empty_scope_entry(self):
        result, output = self.run_check(
            self.coverage([("case", ["Scoped"])]),
            self.scope([("Scoped", "implemented"), ("Empty", "empty")]),
        )
        self.assertEqual(result, 1)
        self.assertIn("1 scoped mozjs entry points are not exercised", output)
        self.assertIn("Empty (empty)", output)

    def test_rejects_empty_entry_outside_the_scope(self):
        result, output = self.run_check(
            self.coverage([("case", ["Scoped", "Empty"])]),
            self.scope([("Scoped", "implemented")]),
        )
        self.assertEqual(result, 1)
        self.assertIn("Empty is classified empty, not implemented", output)

    def test_rejects_stale_scope_status(self):
        result, output = self.run_check(
            self.coverage([("case", ["Scoped"])]),
            self.scope([("Scoped", "empty")]),
        )
        self.assertEqual(result, 1)
        self.assertIn("scope entry Scoped says empty, manifest says implemented", output)

    def test_rejects_scope_entry_missing_from_manifest(self):
        result, output = self.run_check(
            self.coverage([("case", ["Scoped"])]),
            self.scope([("Scoped", "implemented"), ("Unknown", "implemented")]),
        )
        self.assertEqual(result, 1)
        self.assertIn("scope entry Unknown is not in the mozjs manifest", output)


if __name__ == "__main__":
    unittest.main()
