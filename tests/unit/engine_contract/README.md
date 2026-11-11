# engine contract tests

Shared contract tests for the `mozjs` API. SpiderMonkey is the reference implementation: every
case asserts SpiderMonkey behavior, and the same source also runs against the QuickJS shim to
expose divergences. The engine is selected at build time by Servo's `[patch.crates-io]`, so the
test source never names an engine and never uses `mozjs::quickjs` or `qjs::`.

## Scope

The suite currently runs 76 cases over the mozjs surface Servo reaches:

| Area | Cases | Source |
| --- | --- | --- |
| Buffer and typed arrays | 26 | `buffer.rs`, `typed_array.rs` |
| Objects and properties | 11 | `objects.rs`, `objects_extra.rs`, `property.rs` |
| Modules and clone | 11 | `module.rs`, `clone.rs` |
| Strings, eval and stencil | 5 | `strings.rs`, `strings_extra.rs`, `eval.rs`, `eval_ext.rs`, `stencil.rs` |
| Promises, realm and runtime | 5 | `promises.rs`, `realm.rs`, `runtime.rs` |
| Class, misc and errors | 5 | `class.rs`, `misc.rs`, `errors.rs` |
| Elements, calls and rooters | 5 | `contract.rs` |
| Proxies, realms and principals | 8 | `proxy.rs` |

Every case records the `apis` it calls. The coverage gate compares those names with
`api-manifest.toml`; the current release run covers all 631 `implemented` entries.

Known-unimplemented entries are outside this suite's scope until their capability is enabled.

## Reachability scope

`contract-scope.toml` records the mozjs entries Servo production code actually reaches. It is
generated from source, not hand maintained:

```bash
python3 tests/unit/engine_contract/tools/scan_scope.py export \
  --servo-dir . \
  --manifest tests/unit/engine_contract/api-manifest.toml \
  --covered /path/to/servo-quickjs/target/contract/qjs.jsonl \
  --output tests/unit/engine_contract/contract-scope.toml
```

Each entry has `path:line` provenance from a resolved call site. Comments, string literals,
`#[cfg(test)]` modules, local functions with a matching name, and Servo's own wrappers are not
reported.

The scope is layered:

- `coverage = "covered"`: a contract case guarantees the entry, and `check_scope.py` fails if it
  is no longer covered.
- `coverage = "pending"`: the entry is reachable but has no case yet. The gate reports the count
  and first provenance location; it does not fail.

Validate a generated scope without regenerating it:

```bash
python3 tests/unit/engine_contract/tools/scan_scope.py check \
  --servo-dir . \
  --manifest tests/unit/engine_contract/api-manifest.toml \
  --scope tests/unit/engine_contract/contract-scope.toml

python3 -m unittest discover -s tests/unit/engine_contract/tools/tests
```

## Engine policy

- This suite is Servo's source of engine contract cases, and every case must pass on
  SpiderMonkey. It is the diagnostic reference, not the QuickJS upgrade gate.
- The QuickJS upgrade gate lives in `servo-quickjs/engine-contract`: `qjs-baseline.jsonl` is the
  strong gate, and a SpiderMonkey difference is review evidence rather than an automatic failure.
- Do not weaken an assertion to make a shim green. Accepted QuickJS behavior changes are recorded
  in the QJS baseline after review.

## Running

```bash
tests/unit/engine_contract/tools/run.sh sm       # SpiderMonkey reference
tests/unit/engine_contract/tools/run.sh qjs      # local servo-quickjs checkout
tests/unit/engine_contract/tools/run.sh diff
tests/unit/engine_contract/tools/run.sh coverage
```

Set `QJS_DIR` if `servo-quickjs` is not next to this checkout. Set `OUT_DIR` to change where the
JSONL files are written. The script always uses absolute paths for `CONTRACT_OUT`.

When `CONTRACT_OUT` is unset the tests still assert their expectations, but no JSONL is written.
The file is truncated at the start of each run and appended per case.

## Comparing

Each case records its normalized value, the pending exception name, and the `apis` it calls.
`compare.py` ignores the `engine` field. `known-differences.toml` remains for the legacy
SpiderMonkey/QuickJS comparison; the QuickJS upgrade gate is the baseline checker in
`servo-quickjs/engine-contract/tools/check_baseline.py`.
