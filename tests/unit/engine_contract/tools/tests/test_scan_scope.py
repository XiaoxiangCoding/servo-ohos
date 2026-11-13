import os
import sys
import tempfile
import textwrap
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import scan_scope


class ScannerTest(unittest.TestCase):
    def write(self, root, relative, source):
        path = os.path.join(root, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as file:
            file.write(textwrap.dedent(source))

    def scan(self, files, manifest_names=("JS_NewPlainObject", "Construct1")):
        with tempfile.TemporaryDirectory() as root:
            self.write(root, "components/crate/Cargo.toml", "")
            for relative, source in files.items():
                self.write(root, relative, source)
            manifest = {name: [{"name": name, "status": "implemented"}] for name in manifest_names}
            return {entry["api"]: entry for entry in scan_scope.scan(root, manifest)}

    def test_comments_and_strings_do_not_count(self):
        found = self.scan(
            {
                "components/crate/lib.rs": """
                    // Construct1(cx)
                    /* JS_NewPlainObject(cx) */
                    fn f() {
                        let _ = "Construct1(cx)";
                    }
                """
            }
        )
        self.assertEqual(found, {})

    def test_test_module_does_not_count(self):
        found = self.scan(
            {
                "components/crate/lib.rs": """
                    #[cfg(test)]
                    mod tests {
                        use js::jsapi::JS_NewPlainObject;
                        fn f() { JS_NewPlainObject(); }
                    }
                """
            }
        )
        self.assertEqual(found, {})

    def test_direct_and_aliased_imports_count(self):
        found = self.scan(
            {
                "components/crate/lib.rs": """
                    use js::jsapi::{JS_NewPlainObject as plain};
                    use js::rust::wrappers2::Construct1;

                    fn f() {
                        plain();
                        Construct1();
                    }
                """
            }
        )
        self.assertEqual(found["JS_NewPlainObject"]["provenance"][0]["line"], 6)
        self.assertEqual(found["Construct1"]["provenance"][0]["line"], 7)

    def test_reexport_is_followed(self):
        found = self.scan(
            {
                "components/crate/lib.rs": """
                    mod reexport {
                        pub(crate) use js::jsapi::JS_NewPlainObject;
                    }
                    use crate::reexport::JS_NewPlainObject;

                    fn f() { JS_NewPlainObject(); }
                """
            }
        )
        self.assertIn("JS_NewPlainObject", found)

    def test_local_function_with_same_name_does_not_count(self):
        found = self.scan(
            {
                "components/crate/lib.rs": """
                    fn Construct1() {}
                    fn f() { Construct1(); }
                """
            }
        )
        self.assertEqual(found, {})

    def test_manifest_status_is_preserved(self):
        found = self.scan(
            {
                "components/crate/lib.rs": """
                    use js::jsapi::Empty;
                    fn f() { Empty(); }
                """
            },
            manifest_names=("Empty",),
        )
        with tempfile.TemporaryDirectory() as root:
            self.write(root, "components/crate/Cargo.toml", "")
            self.write(root, "components/crate/lib.rs", "use js::jsapi::Empty;\nfn f() { Empty(); }\n")
            report = scan_scope.scan(root, {"Empty": [{"name": "Empty", "status": "empty"}]})
        self.assertEqual(report[0]["status"], "empty")


if __name__ == "__main__":
    unittest.main()
