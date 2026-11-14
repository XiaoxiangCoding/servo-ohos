# engine contract tests

Shared contract tests for the `mozjs` API. SpiderMonkey is the reference implementation: every
case asserts SpiderMonkey behavior, and the same source also runs against the QuickJS shim to
expose divergences. The engine is selected at build time by Servo's `[patch.crates-io]`, so the
test source never names an engine and never uses `mozjs::quickjs` or `qjs::`.

## Naming

There are three names to read in a command or path:

- **Commands** use `<verb>-<object>`: the verb is what happens, the object is the input. Examples:
  `run-spidermonkey-contract` runs the contract cases on SpiderMonkey;
  `scan-mozjs-surface` scans both the mozjs API and Servo usage surfaces.
- **Checked-in files** use `<object>-<role>`: the object is what it describes, the role is how it is
  used. Examples: `mozjs-api-snapshot.toml` is the mozjs API snapshot;
  `quickjs-difference-allowlist.toml` is the allowlist of accepted QuickJS differences.
- **Run outputs** use `<producer>-<role>`: `spidermonkey-contract-results.jsonl` is the
  SpiderMonkey contract result; `quickjs-contract-cases.json` is the QuickJS case set that
  produced it.

The main commands and their files:

| Command | Reads | Writes |
| --- | --- | --- |
| `scan-mozjs-surface` | bindgen + shim, then Servo source | `mozjs-api-snapshot.toml`, `mozjs-api-changes.toml`, `servo-mozjs-api-usage.toml` |
| `check-servo-mozjs-api-usage` | Servo source + `servo-mozjs-api-usage.toml` | nothing (check only) |
| `run-spidermonkey-contract` | contract cases | `spidermonkey-contract-results.jsonl` |
| `run-quickjs-contract` | contract cases | `quickjs-contract-results.jsonl` |
| `compare-engine-results` | both result files + `quickjs-difference-allowlist.toml` | terminal report |
| `check-quickjs-shim-fixes` | allowlist + `quickjs-shim-fix-records.md` | nothing (check only) |
| `check-contract-case-coverage` | inventory + usage + SpiderMonkey results | nothing (check only) |
| `run-contract-gate` | all of the above | results, comparison, coverage |

Internal commands, only for troubleshooting one stage:

| Command | Purpose |
| --- | --- |
| `scan-mozjs-api` | only refresh the mozjs snapshot and change report |
| `scan-servo-mozjs-api-usage` | only refresh the Servo usage file |
| `generate-mozjs-api-snapshot` | only regenerate `mozjs-api-snapshot.toml` |
| `check-mozjs-api-snapshot` | only verify the snapshot is fresh |
| `report-mozjs-api-changes` | only compare two snapshots and write the changes |

Legacy names are accepted as compatibility aliases: `sm`, `qjs`, `diff`, `api-scan`, `generate`,
`manifest`, `scope`, `scope-check`, `coverage`, `report`, `fix-check`, `run-sm`, `run-qjs`,
`compare-engines`, `scan-mozjs-api-inventory`, `scan-servo-usage`, `check-servo-usage`,
`check-contract-coverage`, `check-quickjs-fixes`, `generate-mozjs-inventory`,
`check-mozjs-inventory`, `report-mozjs-changes`, and `all`. Prefer the names above.

## Coverage scope

The suite currently runs 58 cases. Coverage is measured against `servo-mozjs-api-usage.toml`: the mozjs
entries Servo production code actually calls directly or reaches through the mozjs wrappers.
The current scope has 410 unique names (313 marked direct, 97 marked indirect), of which 367 are
`implemented`. Every scoped entry has a contract case: the 367 `implemented` entries are
checked as normal coverage, and the remaining 43 (37 `empty`, 6 `todo`) are required cases whose
shim behavior is reported separately. Such a case still asserts SpiderMonkey behavior; on QuickJS
the silent no-op or `qjs_todo!` panic is recorded and must be settled by an entry in
`quickjs-difference-allowlist.toml`, so a missing capability can never silently drop its case.

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
`mozjs-api-snapshot.toml` for status and with `servo-mozjs-api-usage.toml` for the required set.

Cases may exercise entries outside the scope; those entries are ignored for coverage, but every
case must exercise at least one scoped entry. `implemented` entries outside the scope do not need
a case.

Scoped `empty` and `todo` entries must also have a case. They are counted separately, and a case
may declare them even though the manifest does not classify them as `implemented`; outside the
scope the same entries are rejected. This is what keeps an unimplemented
entry visible: dropping the case would fail `coverage`, while keeping it records today's behavior
for the diff.

`servo-mozjs-api-usage.toml` is generated, not hand maintained. Entries reached only through mozjs wrapper
methods, generic `TypedArray` helpers, generated bindings or callback registrations are not
visible to the conservative call scan; each one is listed in `tools/servo-mozjs-api-usage-reviewed.toml` with
its review evidence, and the generator merges that list into the scope. It is the usage standard: a mozjs upgrade
that adds entries does not fail coverage unless Servo starts using them. If Servo gains a new
call site, `check-servo-mozjs-api-usage` fails until the usage file and any newly required case are updated.

`check-contract-case-coverage` first checks that `mozjs-api-snapshot.toml` still matches the bindgen and
QuickJS shim, then checks that `servo-mozjs-api-usage.toml` still matches Servo's usage, and finally
checks cases against the scope. A bindgen addition, removal, status change, or signature change
requires regenerating the inventory; it does not require a case by itself. The signature records the parameter and return
types, so changing a type, adding or removing a parameter, or reordering same-typed parameters
fails; renaming a parameter does not. Line-number and file movements in the shim do not fail this
check.

The public scan command refreshes both sides in dependency order. Use it after a mozjs upgrade or
after adding or removing a mozjs call in Servo:

```bash
tests/unit/engine_contract/tools/run.sh scan-mozjs-surface
tests/unit/engine_contract/tools/run.sh check-servo-mozjs-api-usage
```

`mozjs-api-snapshot.toml` is generated, not edited by hand. When a mozjs upgrade or a shim change alters
the entry points, regenerate it, then regenerate the scope and add cases for every newly used
scoped entry, including `empty`/`todo` entries that are not implemented yet:

```bash
tests/unit/engine_contract/tools/run.sh scan-mozjs-surface
tests/unit/engine_contract/tools/run.sh run-spidermonkey-contract
tests/unit/engine_contract/tools/run.sh check-contract-case-coverage
```

`scan-mozjs-surface` is the only public scan entry: it runs the mozjs scan and then the Servo usage
scan, so the usage file is always classified against the snapshot that was just generated. It
regenerates `mozjs-api-snapshot.toml`, verifies it against the current bindgen and shim, and writes the changes
relative to `HEAD` to `target/contract/mozjs-api-changes.toml`. It reads the bindgen files, engine
sources, mozjs version and shim commit from `QJS_DIR`, so `QJS_DIR` must point at the same checkout
that the build uses. Review the manifest diff, then update the scope and cases; do not edit the
TOML files by hand. Use `--baseline REV:PATH`, `--output FILE`, and `--format tsv` to select an
older baseline, a different output path, or a spreadsheet-friendly table.

`generate-mozjs-api-snapshot`, `check-mozjs-api-snapshot`, and `report-mozjs-api-changes` remain available
for debugging the individual stages.

## Tool tests

Run the manifest tool tests with:

```bash
python3 -m unittest discover -s tests/unit/engine_contract/tools/tests
```

## Engine policy

- This suite is Servo's source of engine contract cases, and every case must pass on
  SpiderMonkey. SpiderMonkey is the reference implementation.
- The QuickJS gate is `run.sh run-quickjs-contract` followed by `run.sh compare-engine-results`: an unlisted difference fails,
  while a difference recorded in `quickjs-difference-allowlist.toml` with status `accepted` or `wontfix`
  is reported but allowed. `pending` and `bug` entries still fail.
- Do not weaken an assertion to make a shim green. Accepted QuickJS behavior changes are recorded
  in `quickjs-difference-allowlist.toml` after review, per field, and the entry is removed once fixed.

When a difference is fixed, add a record to `quickjs-shim-fix-records.md` before deleting its
`quickjs-difference-allowlist.toml` entry. The record names the case, the shim commit, the change, and the
regression command/result, so the A4 fix-to-regression step leaves an auditable trace.

## Running

```bash
tests/unit/engine_contract/tools/run.sh run-spidermonkey-contract # SpiderMonkey reference
tests/unit/engine_contract/tools/run.sh run-quickjs-contract      # local servo-quickjs checkout
tests/unit/engine_contract/tools/run.sh compare-engine-results
tests/unit/engine_contract/tools/run.sh scan-mozjs-surface # public: snapshot, changes, and Servo usage
tests/unit/engine_contract/tools/run.sh generate-mozjs-api-snapshot # internal: regenerate only the snapshot
tests/unit/engine_contract/tools/run.sh report-mozjs-api-changes # internal: compare two snapshots
tests/unit/engine_contract/tools/run.sh check-mozjs-api-snapshot # internal: snapshot matches the shim and bindgen
tests/unit/engine_contract/tools/run.sh scan-mozjs-api # internal: snapshot and changes only
tests/unit/engine_contract/tools/run.sh scan-servo-mozjs-api-usage # internal: usage only
tests/unit/engine_contract/tools/run.sh check-servo-mozjs-api-usage # servo-mozjs-api-usage.toml matches Servo usage
tests/unit/engine_contract/tools/run.sh check-contract-case-coverage # inventory, usage, and case coverage
tests/unit/engine_contract/tools/run.sh check-quickjs-shim-fixes # removed differences have fix records
```

`scan-mozjs-surface` passes scan options to the mozjs stage; it defaults to
`HEAD:tests/unit/engine_contract/mozjs-api-snapshot.toml` as the
baseline, the working-tree inventory as the current input, and
`$OUT_DIR/mozjs-api-changes.{toml,tsv}` as the output.
All three may be overridden, and `--format tsv` writes a spreadsheet-friendly table:

```bash
tests/unit/engine_contract/tools/run.sh scan-mozjs-surface \
  --baseline origin/main:tests/unit/engine_contract/mozjs-api-snapshot.toml \
  --output target/contract/mozjs-api-changes.tsv --format tsv
```

Run one case or a few cases against either engine:

```bash
tests/unit/engine_contract/tools/run.sh run-spidermonkey-contract --case objects.core
tests/unit/engine_contract/tools/run.sh run-quickjs-contract --case objects.core
tests/unit/engine_contract/tools/run.sh run-spidermonkey-contract --cases objects.core,strings.core
tests/unit/engine_contract/tools/run.sh run-spidermonkey-contract --case objects.core --case strings.core
```

`--case` and `--cases` may be repeated. Values from `--cases` may be separated by commas or
whitespace. The selected ids are passed to the harness as `CONTRACT_CASES`; unselected cases do
not execute. If a selected id does not exist, the run fails and lists every id that produced no
record. Each `run-spidermonkey-contract`/`run-quickjs-contract` run writes its runtime inventory to `$OUT_DIR/spidermonkey-contract-cases.json` or
`$OUT_DIR/quickjs-contract-cases.json` through `CONTRACT_CASES_OUT`; `compare-engine-results` checks each JSONL against its own
inventory and requires the two inventories to agree. A truncated or stale run therefore fails
instead of silently comparing a subset. The inventory is written by the harness when cases
execute, so case ids computed at runtime are included; `case_inventory.py` remains a diagnostic
source scan and is not a gate input. A selected run marks the inventory `"complete": false`, so
`compare-engine-results` does not treat differences for unselected cases as stale.

Set `QJS_DIR` if `servo-quickjs` is not next to this checkout. Set `OUT_DIR` to change where the
JSONL files are written. The script always uses absolute paths for `CONTRACT_OUT`.

When `CONTRACT_OUT` is unset the tests still assert their expectations, but no JSONL is written.
The file is truncated at the start of each `run-spidermonkey-contract`/`run-quickjs-contract` run and appended per case. A selected run
therefore cannot leave an old full-suite JSONL behind.

Cases are isolated from each other. A failed assertion records
`{"fields": {"panic": "<message>"}, "apis": []}` for that case and the run continues with the
next case. The test still fails after every case has run, and the failure summary lists each
failed case, so one engine defect cannot hide the cases that follow it.

Cases for `empty`/`todo` entries record the shim precondition or divergence; the QuickJS diff must
already have the corresponding entry in `quickjs-difference-allowlist.toml`.

CI runs `check-contract-case-coverage` after the unit tests, with `CONTRACT_OUT` pointing at the JSONL of that run. It
checks out the `servo-quickjs` revision pinned in `support/quickjs/cargo-config.toml` and passes it
as `QJS_DIR`, so a mozjs or shim update cannot land with a stale manifest, a stale usage scope, or
a used scoped entry that has no case.

`run.sh run-contract-gate` runs `run-spidermonkey-contract → run-quickjs-contract → compare-engine-results → check-quickjs-shim-fixes → check-contract-case-coverage` without stopping at the first failure. It reports
every failed step at the end and exits non-zero if any step failed, so a known QuickJS failure
still produces `quickjs-contract-results.jsonl`, the diff, and the coverage result. When `--case`/`--cases` is used,
`run-contract-gate` skips coverage because a single case cannot cover the full 410-entry scope; the skip is
reported explicitly.

## Comparing

Each case records its normalized value, the pending exception name, and the `apis` it calls. A
case that panics records a `panic` field instead.
`compare.py` ignores the `engine` field. `quickjs-difference-allowlist.toml` is the only difference list for
the SpiderMonkey/QuickJS comparison: an entry names one case, the fields that may differ, its
`status`, `severity`, `reason` and `owner`. Only `accepted` and `wontfix` release a difference;
`pending` and `bug` fail the run. `severity` is `P0`-`P3`, and a `P0`/`P1` entry cannot be
released as accepted or wontfix. An unlisted or malformed entry fails the run. An optional
`issue` may point at the tracking issue for a difference that is still present; `pending` and
`bug` entries must have one, while accepted/wontfix entries may omit it. `panic` is an optional
exact fingerprint for a panicking case: when present, the QuickJS `panic` text must match it
character for character, and a QuickJS panic on an entry without one is a mismatch. A fixed
difference is removed from this file and recorded in `quickjs-shim-fix-records.md`; `check-quickjs-shim-fixes` rejects a removed
entry without a complete record, and rejects a record whose case id is not in the runtime
inventory. An entry for a case that participated in both runs but no longer differs is stale and
fails `diff`; when both runtime inventories are complete, an entry for a case that no longer
exists is stale too. A selected run only reports the first kind for its selected cases.
