import contextlib
import io
import json
import os
import sys
import tempfile
import textwrap
import unittest


sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import fix_record


BASE = """
[[difference]]
id = "module.one"
fields = ["fields"]
reason = "old"
owner = "servo-quickjs"
status = "accepted"
severity = "P3"

[[difference]]
id = "module.two"
fields = ["fields"]
reason = "still present"
owner = "servo-quickjs"
status = "accepted"
severity = "P3"
"""

CURRENT = """
[[difference]]
id = "module.two"
fields = ["fields"]
reason = "still present"
owner = "servo-quickjs"
status = "accepted"
severity = "P3"
"""

RECORD = """# Fixes

## Template

```markdown
## <case id>
- Date: YYYY-MM-DD
```

## Records

## module.one

- Date: 2026-11-14
- Owner: shim team
- Issue: servo-quickjs#123
- Previous status: accepted
- Previous severity: P3
- Fixed in: abc1234
- Shim change: implemented the missing entry point
- Regression: `run.sh sm --case module.one && run.sh qjs --case module.one && run.sh diff`
- Result: `0 mismatches, 0 unaccepted, 0 missing`
- Follow-up: none
"""


class FixRecordTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)

    def write(self, name, content):
        path = os.path.join(self.temporary.name, name)
        with open(path, "w", encoding="utf-8") as file:
            file.write(textwrap.dedent(content))
        return path

    def run_check(self, baseline=BASE, current=CURRENT, record=RECORD, cases=None):
        baseline_path = self.write("baseline.toml", baseline)
        current_path = self.write("known-differences.toml", current)
        record_path = self.write("fix-record.md", record)
        arguments = [
            "check",
            "--baseline",
            baseline_path,
            "--differences",
            current_path,
            "--record",
            record_path,
        ]
        if cases is not None:
            cases_path = os.path.join(self.temporary.name, "cases.json")
            with open(cases_path, "w", encoding="utf-8") as file:
                json.dump({"version": 1, "cases": cases}, file)
            arguments.extend(["--cases", cases_path])
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            result = fix_record.main(arguments)
        return result, output.getvalue()

    def test_accepts_a_complete_record_for_a_removed_difference(self):
        result, output = self.run_check(cases=["module.one", "module.two"])
        self.assertEqual(result, 0, output)
        self.assertIn("1 removed difference(s), 1 record(s), 0 problems", output)

    def test_rejects_a_removed_difference_without_a_record(self):
        result, output = self.run_check(record="## Records\n")
        self.assertEqual(result, 1)
        self.assertIn("removed difference 'module.one' has no fix record", output)

    def test_ignores_the_fenced_template(self):
        result, output = self.run_check(cases=["module.one", "module.two"])
        self.assertEqual(result, 0, output)

    def test_rejects_a_record_with_a_template_placeholder(self):
        record = RECORD.replace("implemented the missing entry point", "<what changed>")
        result, output = self.run_check(record=record, cases=["module.one", "module.two"])
        self.assertEqual(result, 1)
        self.assertIn("template placeholder in Shim change", output)

    def test_rejects_a_missing_required_field(self):
        record = RECORD.replace("- Result: `0 mismatches, 0 unaccepted, 0 missing`\n", "")
        result, output = self.run_check(record=record, cases=["module.one", "module.two"])
        self.assertEqual(result, 1)
        self.assertIn("missing Result", output)

    def test_rejects_an_unknown_previous_status(self):
        record = RECORD.replace("Previous status: accepted", "Previous status: fixed")
        result, output = self.run_check(record=record, cases=["module.one", "module.two"])
        self.assertEqual(result, 1)
        self.assertIn("unknown previous status 'fixed'", output)

    def test_rejects_a_record_outside_the_runtime_inventory(self):
        result, output = self.run_check(cases=["module.two"])
        self.assertEqual(result, 1)
        self.assertIn("record 'module.one' is not in the runtime case inventory", output)

    def test_rejects_a_missing_records_section(self):
        with self.assertRaisesRegex(SystemExit, "missing '## Records' section"):
            self.run_check(record="# Fixes\n")


if __name__ == "__main__":
    unittest.main()
