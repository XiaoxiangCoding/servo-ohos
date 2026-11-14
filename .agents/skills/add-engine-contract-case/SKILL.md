---
name: add-engine-contract-case
description: Add or revise a mozjs engine-contract case under tests/unit/engine_contract when Servo starts using a new mozjs entry, an upgrade changes a used entry, or a QuickJS divergence needs a regression. Covers case placement, `apis` evidence, the coverage gate, and QuickJS allowlist/fix-record handling.
---

# Add Engine Contract Case

SpiderMonkey is the reference. The same source also runs against the QuickJS shim. Never weaken
a SpiderMonkey assertion to make QuickJS green.

## Is a case needed?

| Change | Action |
| --- | --- |
| Servo starts calling a new mozjs entry | Add or extend a case that exercises it |
| mozjs upgrade changes only unused entries | No case; refresh the snapshot and review the change table |
| A used entry is removed or changes signature | Update the affected case first |
| QuickJS diverges on a covered case | Keep the case; record the divergence in the allowlist |
| QuickJS is fixed for a covered case | Keep the case; remove the allowlist entry and add a fix record |

Coverage is measured against `servo-mozjs-api-usage.toml`, not the full snapshot. A new bindgen
entry needs no case until Servo uses it.

## Workflow

1. Refresh the surfaces, then read the change table:

   ```bash
   RUN=tests/unit/engine_contract/tools/run.sh
   "$RUN" scan-mozjs-surface
   ```

   `QJS_DIR` must be the `servo-quickjs` checkout the build uses (default `../servo-quickjs`).
   This rewrites the two generated TOML files, so review `git diff`. Read
   `target/contract/mozjs-api-changes.toml`, then the entry's `status` (`implemented` / `empty`
   / `todo`) and `provenance` in `servo-mozjs-api-usage.toml`.

2. Put the case in the existing area file: `buffer_test.rs`, `objects_test.rs`,
   `objects_extra_test.rs`, `property_test.rs`, `strings_test.rs`, `strings_extra_test.rs`,
   `eval_test.rs`, `eval_ext_test.rs`, `stencil_test.rs`, `module_test.rs`, `promises_test.rs`,
   `realm_test.rs`, `runtime_test.rs`, `misc_test.rs`, `errors_test.rs`, `class_test.rs`,
   `clone_test.rs`, `debugger_test.rs`, `event_loop_test.rs`, `streaming_test.rs`,
   `proxy_test.rs`, or `contract_test.rs`. Reuse that file's case-id prefix. A new module needs
   `mod <area>_test;` and `run_module` registration in `lib.rs`.

3. Write the case:

   - `record_case(engine, id, apis, |cx, global| ...)` is the normal path; it creates a fresh
     runtime and global. Use `run_case` / `isolate_case` only when the case needs its own
     runtime or observes a panic/abort.
   - Case ids match `^[a-z][a-z0-9_]*\.[a-z0-9_]+$`, e.g. `property.set_prototype`. Name the
     behavior; do not add another `core`/`misc` catch-all.
   - Record engine-independent values: normalized JS values, exception names, stable counters.
     Do not record pointers, addresses, refcounts, or GC timing.
   - No engine-specific imports (`mozjs::quickjs`, `qjs::`) and no `js-quickjs` branch to make
     an assertion pass.

   ```rust
   record_case(engine, "property.set_prototype", &["JS_NewPlainObject", "JS_SetPrototype"],
       |cx, global| unsafe {
           // ... call the entry, then observe its effect
           Observation::new(vec![("matches", eval_observe(cx, global, "/* ... */"))])
       });
   ```

4. `apis` is the coverage evidence. List every mozjs entry the case actually exercises and
   nothing else, including one reached through a support helper when the assertion depends on
   it (`observe_property` -> `JS_GetProperty`). Adding a name without a new assertion is not
   evidence.

5. Extend an existing case when the entry shares its behavior family and the case already
   asserts that behavior. Create a focused case when the entry has its own failure mode, needs a
   narrower QuickJS allowlist, or would make the parent case too broad. A bindgen addition
   alone never requires a case.

6. `empty` means the shim body is a silent no-op; `todo` means the call panics. Both still
   require a case inside Servo's usage surface. Assert the SpiderMonkey behavior; when the shim
   call would abort the process, guard it and record a documented precondition instead of
   calling it. This is the only accepted engine branch.

7. Verify:

   ```bash
   "$RUN" run-spidermonkey-contract --case <area>.<behavior>
   "$RUN" run-quickjs-contract      --case <area>.<behavior>
   "$RUN" compare-engine-results

   "$RUN" run-spidermonkey-contract
   "$RUN" check-contract-case-coverage
   # or: "$RUN" run-contract-gate
   ```

   A selected run writes `"complete": false`; it does not prove full coverage. The full
   SpiderMonkey run plus the coverage check (or the gate) is the acceptance check.

8. Gate behavior:

   - `mozjs-api-snapshot.toml` and `servo-mozjs-api-usage.toml` are generated; never hand-edit.
   - The scanner cannot see wrapper methods, generic `TypedArray` helpers, generated bindings,
     or callback registrations. Add those to `tools/servo-mozjs-api-usage-reviewed.toml` with
     `mode` and concrete `evidence`.
   - Every `implemented` scoped entry needs a case; `empty`/`todo` scoped entries are counted
     separately but also required. A case must include at least one scoped entry, and may name
     `empty`/`todo` entries only when they are in scope.
   - `quickjs-difference-allowlist.toml` is hand-maintained, one entry per case. Only `accepted`
     and `wontfix` release a difference; `pending` and `bug` fail. `P0`/`P1` cannot be
     released. Narrow `fields`; for an abort, use an exact `panic` fingerprint.
   - A removed allowlist entry needs a complete record in `quickjs-shim-fix-records.md`:
     `Date`, `Owner`, `Issue`, `Previous status`, `Previous severity`, `Fixed in`,
     `Shim change`, `Regression`, `Result`, `Follow-up`. Then run
     `check-quickjs-shim-fixes`.

## Correcting an existing case

- SpiderMonkey behavior changed intentionally: update the `Observation`, keep the case id.
- QuickJS diverged: do not change the SpiderMonkey expectation; add or narrow the allowlist
  entry.
- The case itself was wrong (bad assertion, unstable field, missing `apis`): fix the case.

Never rewrite an assertion to make QuickJS pass, and never delete a case to remove a difference.

## Version numbers after an upgrade

1. `Cargo.toml`: `js = { package = "mozjs", version = "=<version>", ... }` is hand-edited.
2. `support/quickjs/cargo-config.toml`: bump `rev` when the shim moves. The shim's
   `mozjs/Cargo.toml` version must exactly equal Servo's `mozjs` pin or `[patch.crates-io]`
   will not apply.
3. `mozjs-api-snapshot.toml`'s `mozjs_version` and `quickjs_revision` are generated from
   `QJS_DIR` by `scan-mozjs-surface`; never hand-edit them.

## Before submitting

- Case id and prefix follow the area, and the case actually asserts the new behavior.
- `apis` lists only real calls and covers every scoped entry the case claims.
- Fields are deterministic and engine-independent.
- SpiderMonkey passes; any QuickJS difference is fixed or explicitly allowlisted.
- A full coverage check passes, not only the single-case run.
- Run `./mach fmt`; run the tool tests if you touched `tools/`:

  ```bash
  python3 -m unittest discover -s tests/unit/engine_contract/tools/tests -p 'test_*.py'
  ```
