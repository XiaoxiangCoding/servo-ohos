# engine contract tests

Shared contract tests for the `mozjs` API. SpiderMonkey is the reference implementation: every
case asserts SpiderMonkey behavior, and the same source also runs against the QuickJS shim to
expose divergences. The engine is selected at build time by Servo's `[patch.crates-io]`, so the
test source never names an engine and never uses `mozjs::quickjs` or `qjs::`.

## Scope

The suite currently runs 68 cases over the mozjs surface Servo reaches:

| Area | Cases | Source |
| --- | --- | --- |
| Buffer and typed arrays | 26 | `buffer.rs`, `typed_array.rs` |
| Objects and properties | 11 | `objects.rs`, `objects_extra.rs`, `property.rs` |
| Modules and clone | 11 | `module.rs`, `clone.rs` |
| Strings, eval and stencil | 5 | `strings.rs`, `strings_extra.rs`, `eval.rs`, `eval_ext.rs`, `stencil.rs` |
| Promises, realm and runtime | 5 | `promises.rs`, `realm.rs`, `runtime.rs` |
| Class, misc and errors | 5 | `class.rs`, `misc.rs`, `errors.rs` |
| Elements, calls and rooters | 5 | `contract.rs` |

Every case records the `apis` it calls. The coverage gate compares those names with
`api-manifest.toml`; the current release run covers 569 of 631 `implemented` entries, with the
remaining 62 in `engine/proxy.rs` (38 reachable from Servo, 24 not called by Servo).

Known-unimplemented entries are outside this suite's scope until their capability is enabled.

## Engine policy

- This suite is Servo's source of engine contract cases, and every case must pass on
  SpiderMonkey. It is the diagnostic reference, not the QuickJS upgrade gate.
- The QuickJS upgrade gate lives in `servo-quickjs/engine-contract`: `qjs-baseline.jsonl` is the
  strong gate, and a SpiderMonkey difference is review evidence rather than an automatic failure.
- Do not weaken an assertion to make a shim green. Accepted QuickJS behavior changes are recorded
  in the QJS baseline after review.

## Running

```bash
# SpiderMonkey
CONTRACT_ENGINE=spidermonkey CONTRACT_OUT=target/contract/sm.jsonl \
  cargo test --release -p engine_contract_tests

# QuickJS, pinned shim
CONTRACT_ENGINE=quickjs CONTRACT_OUT=target/contract/qjs.jsonl \
  cargo test --release -p engine_contract_tests \
  --config support/quickjs/cargo-config.toml

# QuickJS, local servo-quickjs checkout
CONTRACT_ENGINE=quickjs CONTRACT_OUT=target/contract/qjs.jsonl \
  cargo test --release -p engine_contract_tests --features js-quickjs \
  --config 'patch.crates-io.mozjs.path="../servo-quickjs/mozjs"'

python3 tests/unit/engine_contract/tools/compare.py \
  target/contract/sm.jsonl target/contract/qjs.jsonl \
  tests/unit/engine_contract/known-differences.toml
```

When `CONTRACT_OUT` is unset the tests still assert their expectations, but no JSONL is written.
The file is truncated at the start of each run and appended per case.

## Comparing

Each case records its normalized value, the pending exception name, and the `apis` it calls.
`compare.py` ignores the `engine` field. `known-differences.toml` remains for the legacy
SpiderMonkey/QuickJS comparison; the QuickJS upgrade gate is the baseline checker in
`servo-quickjs/engine-contract/tools/check_baseline.py`.
