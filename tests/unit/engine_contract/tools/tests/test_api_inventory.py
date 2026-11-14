import contextlib
import io
import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
import types
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import api_inventory


STUB_SOURCE = r'''
import re

FN = re.compile(
    r"(?s)^(?P<attrs>.*?)(?P<vis>pub )fn (?P<name>\w+)\s*\((?P<params>.*)\)"
    r"(?P<ret>\s*->\s*.*?)?;\s*$"
)


def skip_string(text, i):
    i += 1
    while text[i] != '"':
        i += 2 if text[i] == "\\" else 1
    return i + 1


def find_block_end(text, start):
    depth = 0
    i = start
    while True:
        char = text[i]
        if char == '"':
            i = skip_string(text, i)
            continue
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1


def split_items(body):
    items = []
    depth = 0
    current = []
    i = 0
    while i < len(body):
        char = body[i]
        if char == '"':
            end = skip_string(body, i)
            current.append(body[i:end])
            i = end
            continue
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        current.append(char)
        if char == ";" and depth == 0:
            items.append("".join(current))
            current = []
        i += 1
    assert not "".join(current).strip()
    return items


def param_names(params):
    names = []
    depth = 0
    current = ""
    for char in params + ",":
        if char in "([<{":
            depth += 1
        elif char in ")]>}":
            depth -= 1
        if char == "," and depth == 0:
            if current.strip():
                names.append(current.split(":", 1)[0].strip())
            current = ""
        else:
            current += char
    return names
'''


class ManifestVerifyTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = self.temporary.name
        self.bindgen = os.path.join(self.root, "mozjs", "upstream-bindgen")
        self.engine = os.path.join(self.root, "mozjs", "src", "engine")
        self.stub = os.path.join(self.root, "mozjs", "stub_bindings.py")
        self.manifest = os.path.join(self.root, "mozjs-api-snapshot.toml")
        self.write(self.stub, STUB_SOURCE)
        self.write(
            os.path.join(self.bindgen, "jsapi.rs"),
            """
            unsafe extern "C" {
                pub fn Existing(x: i32) -> i32;
            }
            """,
        )
        self.write(os.path.join(self.bindgen, "gluebindings.rs"), "")
        self.write_engine(2)

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, path, source):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as file:
            file.write(textwrap.dedent(source))

    def write_engine(self, blank_lines, body="x"):
        self.write(
            os.path.join(self.engine, "object.rs"),
            "\n" * blank_lines
            + textwrap.dedent(
                f"""
                pub(crate) unsafe fn Existing(x: i32) -> i32 {{
                    {body}
                }}
                """
            ),
        )

    def generate(self):
        with contextlib.redirect_stdout(io.StringIO()):
            api_inventory.generate(
                types.SimpleNamespace(
                    bindgen_dir=self.bindgen,
                    engine_src=self.engine,
                    stub_bindings=self.stub,
                    output=self.manifest,
                    mozjs_version="0.26.14",
                    quickjs_revision="test",
                )
            )

    def verify(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            result = api_inventory.verify(
                types.SimpleNamespace(
                    snapshot=self.manifest,
                    bindgen_dir=self.bindgen,
                    engine_src=self.engine,
                    stub_bindings=self.stub,
                )
            )
        return result, output.getvalue()

    def test_verify_accepts_a_fresh_manifest(self):
        self.generate()
        result, output = self.verify()
        self.assertEqual(result, 0)
        self.assertIn("mozjs API inventory ok: 1 entries", output)

    def test_verify_ignores_moved_shim_lines(self):
        self.generate()
        self.write_engine(20)
        result, output = self.verify()
        self.assertEqual(result, 0, output)

    def test_verify_reports_a_new_bindgen_entry(self):
        self.generate()
        self.write(
            os.path.join(self.bindgen, "jsapi.rs"),
            """
            unsafe extern "C" {
                pub fn Existing(x: i32) -> i32;
                pub fn NewEntry(y: i32) -> i32;
            }
            """,
        )
        result, output = self.verify()
        self.assertEqual(result, 1)
        self.assertIn("+ NewEntry(1) [jsapi]: todo", output)

    def test_verify_reports_a_changed_status(self):
        self.generate()
        self.write_engine(2, body="")
        result, output = self.verify()
        self.assertEqual(result, 1)
        self.assertIn("~ Existing(1) [jsapi]: status implemented -> empty", output)

    def test_constant_scalar_out_parameter_is_an_empty_body(self):
        function = textwrap.dedent(
            """
            pub(crate) unsafe fn Existing(valueOut: *mut u32) -> bool {
                let _ = valueOut;
                *valueOut = 0;
                true
            }
            """
        )
        self.write(os.path.join(self.engine, "object.rs"), function)
        functions = api_inventory.engine_functions(self.engine)
        self.assertTrue(functions[("Existing", 1)][0]["empty"])

    def test_non_constant_scalar_out_parameter_is_not_empty(self):
        self.assertFalse(api_inventory.is_empty_body("*valueOut = 1 + 1; true", {"valueOut"}))

    def test_scalar_out_parameter_without_matching_name_is_not_empty(self):
        self.assertFalse(api_inventory.is_empty_body("*other = 0; true", {"valueOut"}))

    def test_verify_ignores_renamed_parameters(self):
        self.generate()
        self.write(
            os.path.join(self.bindgen, "jsapi.rs"),
            """
            unsafe extern "C" {
                pub fn Existing(renamed: i32) -> i32;
            }
            """,
        )
        result, output = self.verify()
        self.assertEqual(result, 0, output)

    def test_verify_reports_a_changed_parameter_type(self):
        self.generate()
        self.write(
            os.path.join(self.bindgen, "jsapi.rs"),
            """
            unsafe extern "C" {
                pub fn Existing(x: u64) -> i32;
            }
            """,
        )
        result, output = self.verify()
        self.assertEqual(result, 1)
        self.assertIn("~ Existing(1) [jsapi]: signature", output)
        self.assertIn("recorded:  (i32) -> i32", output)
        self.assertIn("generated: (u64) -> i32", output)

    def test_verify_reports_a_changed_return_type(self):
        self.generate()
        self.write(
            os.path.join(self.bindgen, "jsapi.rs"),
            """
            unsafe extern "C" {
                pub fn Existing(x: i32) -> u64;
            }
            """,
        )
        result, output = self.verify()
        self.assertEqual(result, 1)
        self.assertIn("signature", output)
        self.assertIn("recorded:  (i32) -> i32", output)
        self.assertIn("generated: (i32) -> u64", output)

    def test_verify_reports_reordered_parameters_with_the_same_types(self):
        self.generate()
        self.write(
            os.path.join(self.bindgen, "jsapi.rs"),
            """
            unsafe extern "C" {
                pub fn Existing(x: i32, y: i32) -> i32;
            }
            """,
        )
        self.generate()
        self.write(
            os.path.join(self.bindgen, "jsapi.rs"),
            """
            unsafe extern "C" {
                pub fn Existing(x: i32, y: u64) -> i32;
            }
            """,
        )
        result, output = self.verify()
        self.assertEqual(result, 1)
        self.assertIn("recorded:  (i32, i32) -> i32", output)
        self.assertIn("generated: (i32, u64) -> i32", output)


class ManifestReportTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = self.temporary.name
        self.baseline = os.path.join(self.root, "baseline.toml")
        self.current = os.path.join(self.root, "current.toml")
        self.output = os.path.join(
            self.root, "reports", "nested", "mozjs-api-changes.toml"
        )

    def tearDown(self):
        self.temporary.cleanup()

    @staticmethod
    def metadata(revision):
        return {
            "mozjs_version": "0.26.14",
            "bindgen_sha256": "abc123",
            "quickjs_revision": revision,
            "bindgen_definitions": 3,
            "unique_names": 3,
            "variadic": 0,
            "implemented": 1,
            "empty": 1,
            "todo": 1,
            "eligible": 1,
        }

    @staticmethod
    def entry(name, arity, source, status, signature):
        return {
            "name": name,
            "arity": arity,
            "source": source,
            "status": status,
            "signature": signature,
        }

    def write_fixtures(self):
        api_inventory.write_inventory(
            self.baseline,
            [
                self.entry("RemovedFn", 0, "jsapi", "todo", "() -> ()"),
                self.entry("SignatureFn", 1, "jsapi", "implemented", "(i32) -> i32"),
                self.entry("StatusFn", 1, "jsapi", "todo", "(i32) -> i32"),
            ],
            self.metadata("baseline-rev"),
        )
        api_inventory.write_inventory(
            self.current,
            [
                self.entry("AddedFn", 1, "gluebindings", "empty", "(u8) -> bool"),
                self.entry("SignatureFn", 1, "jsapi", "implemented", "(u64) -> i32"),
                self.entry("StatusFn", 1, "jsapi", "implemented", "(i32) -> i32"),
            ],
            self.metadata("current-rev"),
        )

    def run_report(self, *extra_args):
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            result = api_inventory.main(
                [
                    "report",
                    "--baseline",
                    self.baseline,
                    "--inventory",
                    self.current,
                    "--output",
                    self.output,
                    *extra_args,
                ]
            )
        return result, output.getvalue()

    def test_report_subcommand_writes_toml_by_default(self):
        self.write_fixtures()
        result, output = self.run_report()
        self.assertEqual(result, 0, output)
        self.assertIn("4 changes", output)
        with open(self.output, encoding="utf-8") as file:
            report = file.read()
        self.assertIn('baseline_quickjs_revision = "baseline-rev"', report)
        self.assertIn('current_quickjs_revision = "current-rev"', report)
        self.assertIn("changes = 4", report)
        self.assertIn('kind = "added"', report)
        self.assertIn('kind = "removed"', report)
        self.assertIn('kind = "signature"', report)
        self.assertIn('kind = "status"', report)
        self.assertIn('name = "AddedFn"', report)
        self.assertIn('source = "gluebindings"', report)
        self.assertIn('signature = "(u64) -> i32"', report)
        self.assertIn('before = "todo"', report)
        self.assertIn('after = "implemented"', report)

    def test_report_subcommand_writes_tsv(self):
        self.write_fixtures()
        result, output = self.run_report("--format", "tsv")
        self.assertEqual(result, 0, output)
        with open(self.output, encoding="utf-8") as file:
            lines = file.read().splitlines()
        self.assertEqual(lines[0], "change\tname\tarity\tsource\tstatus\tbefore\tafter")
        self.assertIn("added\tAddedFn\t1\tgluebindings\tempty\t\tempty", lines)
        self.assertIn("removed\tRemovedFn\t0\tjsapi\ttodo\ttodo\t", lines)
        self.assertIn(
            "signature\tSignatureFn\t1\tjsapi\timplemented\t(i32) -> i32\t(u64) -> i32",
            lines,
        )
        self.assertIn("status\tStatusFn\t1\tjsapi\timplemented\ttodo\timplemented", lines)
        self.assertEqual(len(lines), 5)

    def test_run_sh_report_entry_point(self):
        self.write_fixtures()
        script = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "run.sh"
        )
        completed = subprocess.run(
            [
                script,
                "report-mozjs-api-changes",
                "--baseline",
                self.baseline,
                "--inventory",
                self.current,
                "--output",
                self.output,
            ],
            cwd=self.root,
            text=True,
            capture_output=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("4 changes", completed.stdout)
        self.assertTrue(os.path.exists(self.output))

    def test_run_sh_report_defaults_to_tsv_extension(self):
        self.write_fixtures()
        script = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "run.sh"
        )
        report_dir = os.path.join(self.root, "tsv-report")
        completed = subprocess.run(
            [
                script,
                "report-mozjs-api-changes",
                "--baseline",
                self.baseline,
                "--inventory",
                self.current,
                "--format",
                "tsv",
            ],
            cwd=self.root,
            env={**os.environ, "OUT_DIR": report_dir},
            text=True,
            capture_output=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertTrue(
            os.path.exists(os.path.join(report_dir, "mozjs-api-changes.tsv"))
        )

    def test_report_rejects_an_unknown_format(self):
        self.write_fixtures()
        with self.assertRaises(SystemExit) as raised:
            self.run_report("--format", "json")
        self.assertEqual(raised.exception.code, 2)


class ApiScanEntryPointTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = self.temporary.name
        self.repository = os.path.join(self.root, "servo")
        self.contract = os.path.join(self.repository, "tests", "unit", "engine_contract")
        self.tools = os.path.join(self.contract, "tools")
        os.makedirs(self.tools)
        source_tools = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        for name in ("run.sh", "api_inventory.py", "scope_manifest.py", "scan_scope.py"):
            shutil.copy(os.path.join(source_tools, name), os.path.join(self.tools, name))
        with open(
            os.path.join(self.tools, "servo-mozjs-api-usage-reviewed.toml"),
            "w",
            encoding="utf-8",
        ):
            pass

        self.qjs = os.path.join(self.root, "servo-quickjs")
        bindgen = os.path.join(self.qjs, "mozjs", "upstream-bindgen")
        engine = os.path.join(self.qjs, "mozjs", "src", "engine")
        os.makedirs(bindgen)
        os.makedirs(engine)
        self.write(
            os.path.join(self.qjs, "mozjs", "Cargo.toml"),
            '[package]\nname = "mozjs"\nversion = "0.26.14"\n',
        )
        self.write(
            os.path.join(bindgen, "jsapi.rs"),
            """
            unsafe extern "C" {
                pub fn Existing(x: i32) -> i32;
            }
            """,
        )
        self.write(os.path.join(bindgen, "gluebindings.rs"), "")
        self.write(
            os.path.join(engine, "object.rs"),
            """
            pub(crate) unsafe fn Existing(x: i32) -> i32 {
                x
            }
            """,
        )
        self.write(os.path.join(self.qjs, "mozjs", "stub_bindings.py"), STUB_SOURCE)

        self.write(
            os.path.join(self.repository, "components", "script", "Cargo.toml"),
            '[package]\nname = "script"\n',
        )
        self.write(
            os.path.join(self.repository, "components", "script", "lib.rs"),
            """
            use mozjs::jsapi::Existing;
            pub fn call_existing() { unsafe { Existing(1); } }
            """,
        )

        self.baseline = os.path.join(self.root, "baseline.toml")
        self.output = os.path.join(self.root, "reports", "mozjs-api-changes.toml")
        self.write_inventory(
            self.baseline,
            [self.entry("Existing", 1, "jsapi", "implemented", "(u64) -> i32")],
        )

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, path, source):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as file:
            file.write(textwrap.dedent(source))

    @staticmethod
    def entry(name, arity, source, status, signature):
        return {
            "name": name,
            "arity": arity,
            "source": source,
            "status": status,
            "signature": signature,
        }

    def write_inventory(self, path, entries):
        api_inventory.write_inventory(
            path,
            entries,
            {
                "mozjs_version": "0.26.14",
                "bindgen_sha256": "baseline",
                "quickjs_revision": "baseline-rev",
                "bindgen_definitions": len(entries),
                "unique_names": len(entries),
                "variadic": 0,
                "implemented": len(entries),
                "empty": 0,
                "todo": 0,
                "eligible": len(entries),
            },
        )

    def test_internal_scan_mozjs_api_generates_verifies_and_reports_in_one_command(self):
        completed = subprocess.run(
            [
                os.path.join(self.tools, "run.sh"),
                "scan-mozjs-api",
                "--baseline",
                self.baseline,
                "--output",
                self.output,
            ],
            cwd=self.repository,
            env={**os.environ, "QJS_DIR": self.qjs},
            text=True,
            capture_output=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("wrote tests/unit/engine_contract/mozjs-api-snapshot.toml", completed.stdout)
        self.assertIn("mozjs API inventory ok: 1 entries", completed.stdout)
        self.assertIn("1 changes (signature 1)", completed.stdout)

        manifest = os.path.join(self.contract, "mozjs-api-snapshot.toml")
        self.assertTrue(os.path.exists(manifest))
        self.assertTrue(os.path.exists(self.output))
        with open(self.output, encoding="utf-8") as file:
            report = file.read()
        self.assertIn('kind = "signature"', report)
        self.assertIn('name = "Existing"', report)
        self.assertIn('before = "(u64) -> i32"', report)
        self.assertIn('after = "(i32) -> i32"', report)

    def test_internal_scan_mozjs_api_rejects_an_explicit_inventory(self):
        completed = subprocess.run(
            [
                os.path.join(self.tools, "run.sh"),
                "scan-mozjs-api",
                "--baseline",
                self.baseline,
                "--inventory",
                self.baseline,
            ],
            cwd=self.repository,
            env={**os.environ, "QJS_DIR": self.qjs},
            text=True,
            capture_output=True,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("scan-mozjs-api always regenerates", completed.stderr)

    def test_compatibility_aliases_keep_the_same_snapshot_and_change_names(self):
        expected_run = os.path.join(self.contract, "mozjs-api-snapshot.toml")
        expected_report = os.path.join(self.output)
        completed = subprocess.run(
            [
                os.path.join(self.tools, "run.sh"),
                "scan-mozjs-api-inventory",
                "--baseline",
                self.baseline,
                "--output",
                self.output,
            ],
            cwd=self.repository,
            env={**os.environ, "QJS_DIR": self.qjs},
            text=True,
            capture_output=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertTrue(os.path.exists(expected_run))
        self.assertTrue(os.path.exists(expected_report))

    def test_scan_mozjs_surface_updates_the_snapshot_and_servo_usage_together(self):
        bin_dir = os.path.join(self.root, "bin")
        os.makedirs(bin_dir)
        fake_cargo = os.path.join(bin_dir, "cargo")
        with open(fake_cargo, "w", encoding="utf-8") as file:
            file.write(
                "#!/usr/bin/env bash\n"
                "set -eu\n"
                "python3 - \"$@\" <<'PY'\n"
                "import json, os, sys\n"
                "root = os.environ['FAKE_CARGO_ROOT']\n"
                "print(json.dumps({'packages': [\n"
                "    {'name': 'mozjs', 'manifest_path': os.path.join(root, 'mozjs', 'Cargo.toml')},\n"
                "    {'name': 'mozjs_sys', 'manifest_path': os.path.join(root, 'mozjs_sys', 'Cargo.toml')},\n"
                "]}))\n"
                "PY\n"
            )
        os.chmod(fake_cargo, 0o755)
        completed = subprocess.run(
            [
                os.path.join(self.tools, "run.sh"),
                "scan-mozjs-surface",
                "--baseline",
                self.baseline,
                "--output",
                self.output,
            ],
            cwd=self.repository,
            env={
                **os.environ,
                "QJS_DIR": self.qjs,
                "FAKE_CARGO_ROOT": self.root,
                "PATH": bin_dir + os.pathsep + os.environ["PATH"],
            },
            text=True,
            capture_output=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("wrote tests/unit/engine_contract/mozjs-api-snapshot.toml", completed.stdout)
        self.assertIn("servo-mozjs-api-usage.toml", completed.stdout)
        self.assertTrue(os.path.exists(os.path.join(self.contract, "mozjs-api-snapshot.toml")))
        usage = os.path.join(self.contract, "servo-mozjs-api-usage.toml")
        self.assertTrue(os.path.exists(usage))
        with open(usage, encoding="utf-8") as file:
            self.assertIn('name = "Existing"', file.read())


if __name__ == "__main__":
    unittest.main()
