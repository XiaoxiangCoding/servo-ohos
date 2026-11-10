# engine contract tests

Shared contract tests for the `mozjs` API. SpiderMonkey is the reference implementation: every
case asserts SpiderMonkey behavior, and the same source also runs against the QuickJS shim to
expose divergences. The engine is selected at build time by Servo's `[patch.crates-io]`, so the
test source never names an engine and never uses `mozjs::quickjs` or `qjs::`.

## Scope

The first slice covers 10 cases in 5 capability areas:

| Area | Cases |
| --- | --- |
| Property | `property.get_missing`, `property.set_then_get`, `property.define_readonly` |
| Element | `element.set_then_get`, `element.script_view` |
| Call | `call.return_value`, `call.throw_pending_exception` |
| Buffer | `buffer.new_and_write`, `buffer.detach` |
| Rooter | `rooter.trace_on_gc` |

Known-unimplemented entries are outside this suite's scope until their capability is enabled.

## Engine policy

- SpiderMonkey is the contract. A case must pass on SpiderMonkey.
- The QuickJS run must also pass for unwrapped behavior; a shim divergence is a QuickJS defect.
- `known-differences.toml` records each accepted defect by case and field, with an owner and
  reason. The comparison still prints every known defect and fails on anything unlisted.
- Do not change the shim or weaken an assertion to make this suite green.

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

Each case records its normalized value, the pending exception name, and nothing else. The
comparison ignores the `engine` field. A difference listed for that case and field in
`known-differences.toml` is printed as a known QuickJS defect and does not fail the run; every
other difference does.
