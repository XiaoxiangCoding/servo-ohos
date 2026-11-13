import collections
import os
import sys
import unittest


sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import scope_manifest


class ReviewedUsageTest(unittest.TestCase):
    def test_reviewed_usage_adds_direct_and_indirect_entries(self):
        direct = collections.defaultdict(list)
        reached = collections.defaultdict(set)
        reviewed = {
            "JS_InitializePropertiesFromCompatibleNativeObject": {
                "mode": "direct",
                "evidence": "reviewed: wrap.rs:134",
            },
            "AppendToEnvironmentChain": {
                "mode": "indirect",
                "evidence": "reviewed: rust.rs:1263",
            },
        }
        entries = {
            "JS_InitializePropertiesFromCompatibleNativeObject",
            "AppendToEnvironmentChain",
        }

        scope_manifest.merge_reviewed_usage(direct, reached, reviewed, entries)

        self.assertEqual(
            direct["JS_InitializePropertiesFromCompatibleNativeObject"],
            ["reviewed: wrap.rs:134"],
        )
        self.assertEqual(reached["AppendToEnvironmentChain"], {"reviewed: rust.rs:1263"})
        self.assertNotIn("AppendToEnvironmentChain", direct)

    def test_unknown_reviewed_entry_is_rejected(self):
        with self.assertRaises(SystemExit):
            scope_manifest.merge_reviewed_usage(
                collections.defaultdict(list),
                collections.defaultdict(set),
                {"NotInManifest": {"mode": "indirect", "evidence": "reviewed: nowhere"}},
                {"JS_NewPlainObject"},
            )


if __name__ == "__main__":
    unittest.main()
