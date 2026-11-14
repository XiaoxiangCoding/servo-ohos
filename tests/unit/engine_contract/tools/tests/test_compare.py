import contextlib
import io
import json
import os
import sys
import tempfile
import textwrap
import unittest


sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import compare


DIFFERENCES_HEADER = """
[[difference]]
id = "case.one"
fields = ["fields"]
reason = "QuickJS differs in this field"
owner = "servo-quickjs"
"""


class CompareTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)

    def write(self, name, content):
        path = os.path.join(self.temporary.name, name)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(textwrap.dedent(content))
        return path

    def coverage(self, name, fields):
        return self.write(
            name,
            json.dumps(
                {
                    "id": "case.one",
                    "fields": fields,
                    "exception": None,
                    "apis": ["JS_Test"],
                }
            )
            + "\n",
        )

    def run_compare(self, status, severity, qjs_fields=None):
        sm = self.coverage("sm.jsonl", {"value": True})
        qjs = self.coverage("qjs.jsonl", qjs_fields or {"value": False})
        issue = 'issue = "servo-quickjs#1"\n' if status in ("pending", "bug") else ""
        differences = self.write(
            "known-differences.toml",
            DIFFERENCES_HEADER
            + issue
            + f'status = "{status}"\n'
            + f'severity = "{severity}"\n',
        )
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            result = compare.main(["compare.py", sm, qjs, differences])
        return result, output.getvalue()

    def test_releases_accepted_difference(self):
        result, output = self.run_compare("accepted", "P3")
        self.assertEqual(result, 0, output)
        self.assertIn("1 known QuickJS defects, 0 unaccepted", output)

    def test_releases_wontfix_difference(self):
        result, output = self.run_compare("wontfix", "P2")
        self.assertEqual(result, 0, output)

    def test_pending_difference_fails(self):
        result, output = self.run_compare("pending", "P3")
        self.assertEqual(result, 1)
        self.assertIn("unaccepted QuickJS defect: case.one", output)

    def test_bug_difference_fails(self):
        result, output = self.run_compare("bug", "P2")
        self.assertEqual(result, 1)
        self.assertIn("status: bug", output)

    def test_p0_and_p1_cannot_be_released(self):
        for severity in ("P0", "P1"):
            with self.subTest(severity=severity):
                with self.assertRaisesRegex(SystemExit, "cannot release"):
                    self.run_compare("accepted", severity)

    def test_difference_requires_reason_owner_status_and_severity(self):
        sm = self.coverage("sm.jsonl", {"value": True})
        qjs = self.coverage("qjs.jsonl", {"value": False})
        differences = self.write(
            "known-differences.toml",
            """
            [[difference]]
            id = "case.one"
            fields = ["fields"]
            """,
        )
        with self.assertRaisesRegex(SystemExit, "missing reason, owner, status, severity"):
            compare.main(["compare.py", sm, qjs, differences])

    def test_rejects_unknown_status(self):
        with self.assertRaisesRegex(SystemExit, "unknown status 'ignore'"):
            self.run_compare("ignore", "P3")

    def test_rejects_unknown_severity(self):
        with self.assertRaisesRegex(SystemExit, "unknown severity 'major'"):
            self.run_compare("accepted", "major")

    def test_pending_difference_requires_an_issue(self):
        sm = self.coverage("sm.jsonl", {"value": True})
        qjs = self.coverage("qjs.jsonl", {"value": False})
        differences = self.write(
            "known-differences.toml",
            DIFFERENCES_HEADER
            + 'status = "pending"\n'
            + 'severity = "P3"\n',
        )
        with self.assertRaisesRegex(SystemExit, "has no tracking issue"):
            compare.main(["compare.py", sm, qjs, differences])

    def test_rejects_an_empty_issue(self):
        sm = self.coverage("sm.jsonl", {"value": True})
        qjs = self.coverage("qjs.jsonl", {"value": False})
        differences = self.write(
            "known-differences.toml",
            DIFFERENCES_HEADER
            + 'issue = ""\n'
            + 'status = "accepted"\n'
            + 'severity = "P3"\n',
        )
        with self.assertRaisesRegex(SystemExit, "empty issue"):
            compare.main(["compare.py", sm, qjs, differences])

    def test_accepts_an_issue_reference(self):
        sm = self.coverage("sm.jsonl", {"value": True})
        qjs = self.coverage("qjs.jsonl", {"value": False})
        differences = self.write(
            "known-differences.toml",
            DIFFERENCES_HEADER
            + 'issue = "servo-quickjs#123"\n'
            + 'status = "accepted"\n'
            + 'severity = "P3"\n',
        )
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            result = compare.main(["compare.py", sm, qjs, differences])
        self.assertEqual(result, 0, output.getvalue())
        self.assertIn("issue: servo-quickjs#123", output.getvalue())

    def test_rejects_duplicate_difference_ids(self):
        sm = self.coverage("sm.jsonl", {"value": True})
        qjs = self.coverage("qjs.jsonl", {"value": False})
        entry = (
            DIFFERENCES_HEADER
            + 'status = "accepted"\n'
            + 'severity = "P3"\n'
        )
        differences = self.write(
            "known-differences.toml",
            entry + entry,
        )
        with self.assertRaisesRegex(SystemExit, "duplicate difference id 'case.one'"):
            compare.main(["compare.py", sm, qjs, differences])

    def inventory(self, cases):
        path = os.path.join(self.temporary.name, "cases.json")
        with open(path, "w", encoding="utf-8") as file:
            json.dump({"version": 1, "cases": cases}, file)
        return path

    def test_requires_every_inventoried_case_in_both_runs(self):
        sm = self.coverage("sm.jsonl", {"value": True})
        qjs = self.coverage("qjs.jsonl", {"value": True})
        cases = self.inventory(["case.one", "case.two"])
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            result = compare.main(["compare.py", sm, qjs, "--cases", cases])
        self.assertEqual(result, 1)
        self.assertIn("missing from SpiderMonkey case inventory: case.two", output.getvalue())
        self.assertIn("missing from QuickJS case inventory: case.two", output.getvalue())

    def test_rejects_cases_outside_the_inventory(self):
        sm = self.coverage("sm.jsonl", {"value": True})
        qjs = self.coverage("qjs.jsonl", {"value": True})
        cases = self.inventory(["case.two"])
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            result = compare.main(["compare.py", sm, qjs, "--cases", cases])
        self.assertEqual(result, 1)
        self.assertIn("unexpected in SpiderMonkey case inventory: case.one", output.getvalue())

    def test_rejects_an_unsupported_case_inventory(self):
        sm = self.coverage("sm.jsonl", {"value": True})
        qjs = self.coverage("qjs.jsonl", {"value": True})
        path = os.path.join(self.temporary.name, "cases.json")
        with open(path, "w", encoding="utf-8") as file:
            json.dump({"version": 2, "cases": ["case.one"]}, file)
        with self.assertRaisesRegex(SystemExit, "unsupported case inventory version"):
            compare.main(["compare.py", sm, qjs, "--cases", path])


if __name__ == "__main__":
    unittest.main()
