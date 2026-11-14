# engine contract tests

Shared contract tests for the `mozjs` API. SpiderMonkey is the reference implementation: every
case asserts SpiderMonkey behavior, and the same source also runs against the QuickJS shim to
expose divergences. The engine is selected at build time by Servo's `[patch.crates-io]`, so the
test source never names an engine and never uses `mozjs::quickjs` or `qjs::`.

## Scope

The suite currently runs 58 cases. Coverage is measured against `api-scope.toml`: the mozjs
entries Servo production code actually calls directly or reaches through the mozjs wrappers.
The current scope has 410 unique names (313 marked direct, 97 marked indirect), of which 367 are
`implemented`. Every scoped entry has a contract case: the 367 `implemented` entries are
checked as normal coverage, and the remaining 43 (37 `empty`, 6 `todo`) are required cases whose
shim behavior is reported separately. Such a case still asserts SpiderMonkey behavior; on QuickJS
the silent no-op or `qjs_todo!` panic is recorded and must be settled by an entry in
`known-differences.toml`, so a missing capability can never silently drop its case.

| Area | Cases | Source |
| --- | --- | --- |
| Buffer and typed arrays | 6 | `buffer_test.rs` |
| Objects and properties | 7 | `objects_test.rs`, `objects_extra_test.rs`, `property_test.rs` |
| Modules and clone | 7 | `module_test.rs`, `clone_test.rs` |
| Strings, eval and stencil | 5 | `strings_test.rs`, `strings_extra_test.rs`, `eval_test.rs`, `eval_ext_test.rs`, `stencil_test.rs` |
| Promises, realm and runtime | 6 | `promises_test.rs`, `realm_test.rs`, `runtime_test.rs` |
| Class, misc and errors | 13 | `class_test.rs`, `misc_test.rs`, `errors_test.rs` |
| Calls and rooters | 2 | `contract_test.rs` |
| Proxies, realms and principals | 8 | `proxy_test.rs` |
| Debugger object | 1 | `debugger_test.rs` |
| Event-loop dispatch | 2 | `event_loop_test.rs` |
| Streaming callbacks | 1 | `streaming_test.rs` |

Every case records the `apis` it calls. The coverage gate compares those names with
`api-manifest.toml` for status and with `api-scope.toml` for the required set.

Cases may exercise entries outside the scope; those entries are ignored for coverage, but every
case must exercise at least one scoped entry. `implemented` entries outside the scope do not need
a case.

Scoped `empty` and `todo` entries must also have a case. They are counted separately, and a case
may declare them even though the manifest does not classify them as `implemented`; outside the
scope the same entries are rejected. This is what keeps an unimplemented
entry visible: dropping the case would fail `coverage`, while keeping it records today's behavior
for the diff.

`api-scope.toml` is generated, not hand maintained. Entries reached only through mozjs wrapper
methods, generic `TypedArray` helpers, generated bindings or callback registrations are not
visible to the conservative call scan; each one is listed in `tools/reviewed_usage.toml` with
its review evidence, and the generator merges that list into the scope. It is the usage standard: a mozjs upgrade
that adds entries does not fail coverage unless Servo starts using them. If Servo gains a new
call site, `scope-check` fails until the scope and any newly required case are updated.

`coverage` first checks that `api-manifest.toml` still matches the bindgen and QuickJS shim, then
checks that `api-scope.toml` still matches Servo's usage, and finally checks cases against the
scope. A bindgen addition, removal, status change, or signature change requires regenerating the
manifest; it does not require a case by itself. The signature records the parameter and return
types, so changing a type, adding or removing a parameter, or reordering same-typed parameters
fails; renaming a parameter does not. Line-number and file movements in the shim do not fail this
check.

Regenerate the usage scope after adding or removing a mozjs call in Servo:

```bash
tests/unit/engine_contract/tools/run.sh scope
tests/unit/engine_contract/tools/run.sh scope-check
```

`api-manifest.toml` is generated, not edited by hand. When a mozjs upgrade or a shim change alters
the entry points, regenerate it, then regenerate the scope and add cases for every newly used
scoped entry, including `empty`/`todo` entries that are not implemented yet:

```bash
tests/unit/engine_contract/tools/run.sh generate
tests/unit/engine_contract/tools/run.sh report
tests/unit/engine_contract/tools/run.sh scope
tests/unit/engine_contract/tools/run.sh sm
tests/unit/engine_contract/tools/run.sh coverage
```

`generate` reads the bindgen files, engine sources, mozjs version and shim commit from `QJS_DIR`,
so `QJS_DIR` must point at the same checkout that the build uses. Review the manifest diff, then
update the scope and cases; do not edit the TOML files by hand. `report` compares the manifest in
`HEAD` with the regenerated working-tree manifest and writes
`target/contract/api-changes.toml`. It records added, removed, status and signature changes, so
the change table can be reviewed alongside the updated scope and cases.

## Tool tests

Run the manifest tool tests with:

```bash
python3 -m unittest discover -s tests/unit/engine_contract/tools/tests
```

## Engine policy

- This suite is Servo's source of engine contract cases, and every case must pass on
  SpiderMonkey. SpiderMonkey is the reference implementation.
- The QuickJS gate is `run.sh qjs` followed by `run.sh diff`: an unlisted difference fails,
  while a difference recorded in `known-differences.toml` with status `accepted` or `wontfix`
  is reported but allowed. `pending` and `bug` entries still fail.
- Do not weaken an assertion to make a shim green. Accepted QuickJS behavior changes are recorded
  in `known-differences.toml` after review, per field, and the entry is removed once fixed.

When a difference is fixed, add a record to `fix-record.md` before deleting its
`known-differences.toml` entry. The record names the case, the shim commit, the change, and the
regression command/result, so the A4 fix-to-regression step leaves an auditable trace.

## Running

```bash
tests/unit/engine_contract/tools/run.sh sm       # SpiderMonkey reference
tests/unit/engine_contract/tools/run.sh qjs      # local servo-quickjs checkout
tests/unit/engine_contract/tools/run.sh diff
tests/unit/engine_contract/tools/run.sh generate # regenerate api-manifest.toml after an API change
tests/unit/engine_contract/tools/run.sh report   # compare HEAD's manifest with the working tree
tests/unit/engine_contract/tools/run.sh manifest # api-manifest.toml matches the shim and bindgen
tests/unit/engine_contract/tools/run.sh scope    # regenerate api-scope.toml from Servo usage
tests/unit/engine_contract/tools/run.sh scope-check # api-scope.toml matches Servo usage
tests/unit/engine_contract/tools/run.sh coverage # manifest, scope, and case coverage
tests/unit/engine_contract/tools/run.sh fix-check # removed differences have fix records
```

`report` defaults to `HEAD:tests/unit/engine_contract/api-manifest.toml` as the baseline, the
working-tree manifest as the current input, and `$OUT_DIR/api-changes.{toml,tsv}` as the output.
All three may be overridden, and `--format tsv` writes a spreadsheet-friendly table:

```bash
tests/unit/engine_contract/tools/run.sh report \
  --baseline origin/main:tests/unit/engine_contract/api-manifest.toml \
  --manifest tests/unit/engine_contract/api-manifest.toml \
  --output target/contract/api-changes.tsv --format tsv
```

Run one case or a few cases against either engine:

```bash
tests/unit/engine_contract/tools/run.sh sm --case objects.core
tests/unit/engine_contract/tools/run.sh qjs --case objects.core
tests/unit/engine_contract/tools/run.sh sm --cases objects.core,strings.core
tests/unit/engine_contract/tools/run.sh sm --case objects.core --case strings.core
```

`--case` and `--cases` may be repeated. Values from `--cases` may be separated by commas or
whitespace. The selected ids are passed to the harness as `CONTRACT_CASES`; unselected cases do
not execute. If a selected id does not exist, the run fails and lists every id that produced no
record. Each `sm`/`qjs` run writes its runtime inventory to `$OUT_DIR/sm-cases.json` or
`$OUT_DIR/qjs-cases.json` through `CONTRACT_CASES_OUT`; `diff` checks each JSONL against its own
inventory and requires the two inventories to agree. A truncated or stale run therefore fails
instead of silently comparing a subset. The inventory is written by the harness when cases
execute, so case ids computed at runtime are included; `case_inventory.py` remains a diagnostic
source scan and is not a gate input.

Set `QJS_DIR` if `servo-quickjs` is not next to this checkout. Set `OUT_DIR` to change where the
JSONL files are written. The script always uses absolute paths for `CONTRACT_OUT`.

When `CONTRACT_OUT` is unset the tests still assert their expectations, but no JSONL is written.
The file is truncated at the start of each `sm`/`qjs` run and appended per case. A selected run
therefore cannot leave an old full-suite JSONL behind.

Cases are isolated from each other. A failed assertion records
`{"fields": {"panic": "<message>"}, "apis": []}` for that case and the run continues with the
next case. The test still fails after every case has run, and the failure summary lists each
failed case, so one engine defect cannot hide the cases that follow it.

Cases for `empty`/`todo` entries record the shim precondition or divergence; the QuickJS diff must
already have the corresponding entry in `known-differences.toml`.

CI runs `coverage` after the unit tests, with `CONTRACT_OUT` pointing at the JSONL of that run. It
checks out the `servo-quickjs` revision pinned in `support/quickjs/cargo-config.toml` and passes it
as `QJS_DIR`, so a mozjs or shim update cannot land with a stale manifest, a stale usage scope, or
a used scoped entry that has no case.

`run.sh all` runs `sm → qjs → diff → fix-check → coverage` without stopping at the first failure. It reports
every failed step at the end and exits non-zero if any step failed, so a known QuickJS failure
still produces `qjs.jsonl`, the diff, and the coverage result. When `--case`/`--cases` is used,
`all` skips coverage because a single case cannot cover the full 410-entry scope; the skip is
reported explicitly.

## Comparing

Each case records its normalized value, the pending exception name, and the `apis` it calls. A
case that panics records a `panic` field instead.
`compare.py` ignores the `engine` field. `known-differences.toml` is the only difference list for
the SpiderMonkey/QuickJS comparison: an entry names one case, the fields that may differ, its
`status`, `severity`, `reason` and `owner`. Only `accepted` and `wontfix` release a difference;
`pending` and `bug` fail the run. `severity` is `P0`-`P3`, and a `P0`/`P1` entry cannot be
released as accepted or wontfix. An unlisted or malformed entry fails the run. An optional
`issue` may point at the tracking issue for a difference that is still present; `pending` and
`bug` entries must have one, while accepted/wontfix entries may omit it. `panic` is an optional
exact fingerprint for a panicking case: when present, the QuickJS `panic` text must match it
character for character, and a QuickJS panic on an entry without one is a mismatch. A fixed
difference is removed from this file and recorded in `fix-record.md`; `fix-check` rejects a removed
entry without a complete record, and rejects a record whose case id is not in the runtime
inventory.
