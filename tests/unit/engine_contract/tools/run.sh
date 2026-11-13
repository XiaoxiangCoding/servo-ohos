#!/usr/bin/env bash
# Run the engine contract suite against SpiderMonkey (the reference) or QuickJS, then compare.
#
# Usage:
#   tools/run.sh sm       [cargo test args...]
#   tools/run.sh qjs      [cargo test args...]
#   tools/run.sh diff
#   tools/run.sh generate   regenerate api-manifest.toml from the current mozjs bindgen and shim
#   tools/run.sh manifest   check api-manifest.toml against the current mozjs bindgen and shim
#   tools/run.sh coverage   check the coverage of api-manifest.toml in sm.jsonl
#   tools/run.sh scope      regenerate api-scope.toml from Servo's actual usage
#   tools/run.sh scope-check  verify api-scope.toml is fresh
#   tools/run.sh all
#
# Environment:
#   QJS_DIR  local servo-quickjs checkout (default: ../servo-quickjs)
#   OUT_DIR  output directory (default: target/contract)

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../../.." && pwd)"
out_dir="${OUT_DIR:-$repo_root/target/contract}"
qjs_dir="${QJS_DIR:-$repo_root/../servo-quickjs}"

# Resolve a caller-supplied relative OUT_DIR against the invoking directory, not the repository
# root or the crate directory that `cargo test` runs from. `sm`, `qjs` and `diff` must agree on it.
case "$out_dir" in
    /*) ;;
    *) out_dir="$(pwd)/$out_dir" ;;
esac

run_sm() {
    mkdir -p "$out_dir"
    cd "$repo_root"
    CONTRACT_ENGINE=spidermonkey CONTRACT_OUT="$out_dir/sm.jsonl" \
        cargo test --release -p engine_contract_tests "$@"
}

run_qjs() {
    mkdir -p "$out_dir"
    cd "$repo_root"
    CONTRACT_ENGINE=quickjs CONTRACT_OUT="$out_dir/qjs.jsonl" \
        cargo test --release -p engine_contract_tests --features js-quickjs \
        --config "patch.crates-io.mozjs.path=\"$qjs_dir/mozjs\"" "$@"
}

run_diff() {
    [[ -f "$out_dir/sm.jsonl" && -f "$out_dir/qjs.jsonl" ]] || {
        echo "missing $out_dir/sm.jsonl or $out_dir/qjs.jsonl; run '$0 sm' and '$0 qjs' first" >&2
        return 1
    }
    cd "$repo_root"
    python3 tests/unit/engine_contract/tools/compare.py \
        "$out_dir/sm.jsonl" "$out_dir/qjs.jsonl" \
        tests/unit/engine_contract/known-differences.toml
}

run_manifest() {
    [[ -f "$qjs_dir/mozjs/upstream-bindgen/jsapi.rs" ]] || {
        echo "missing $qjs_dir/mozjs/upstream-bindgen; set QJS_DIR to a servo-quickjs checkout" >&2
        return 1
    }
    cd "$repo_root"
    python3 tests/unit/engine_contract/tools/api_inventory.py verify \
        --manifest tests/unit/engine_contract/api-manifest.toml \
        --bindgen-dir "$qjs_dir/mozjs/upstream-bindgen" \
        --engine-src "$qjs_dir/mozjs/src/engine" \
        --stub-bindings "$qjs_dir/mozjs/stub_bindings.py"
}

run_generate() {
    [[ -f "$qjs_dir/mozjs/upstream-bindgen/jsapi.rs" ]] || {
        echo "missing $qjs_dir/mozjs/upstream-bindgen; set QJS_DIR to a servo-quickjs checkout" >&2
        return 1
    }
    local mozjs_version quickjs_revision
    mozjs_version=$(sed -nE 's/^version = "([^"]+)".*/\1/p' "$qjs_dir/mozjs/Cargo.toml" | head -n 1)
    quickjs_revision=$(git -C "$qjs_dir" rev-parse --short=7 HEAD 2>/dev/null || echo unknown)
    cd "$repo_root"
    python3 tests/unit/engine_contract/tools/api_inventory.py generate \
        --bindgen-dir "$qjs_dir/mozjs/upstream-bindgen" \
        --engine-src "$qjs_dir/mozjs/src/engine" \
        --stub-bindings "$qjs_dir/mozjs/stub_bindings.py" \
        --output tests/unit/engine_contract/api-manifest.toml \
        --mozjs-version "$mozjs_version" \
        --quickjs-revision "$quickjs_revision"
}

run_coverage() {
    run_manifest
    run_scope_check
    [[ -f "$out_dir/sm.jsonl" ]] || {
        echo "missing $out_dir/sm.jsonl; run '$0 sm' first" >&2
        return 1
    }
    cd "$repo_root"
    python3 tests/unit/engine_contract/tools/check_api_coverage.py \
        "$out_dir/sm.jsonl" tests/unit/engine_contract/api-manifest.toml \
        tests/unit/engine_contract/api-scope.toml
}

locate_mozjs() {
    cd "$repo_root"
    local registries
    registries=$(cargo metadata --locked --offline --format-version 1 | python3 -c '
import json, sys
for package in json.load(sys.stdin)["packages"]:
    if package["name"] in ("mozjs", "mozjs_sys"):
        print(package["name"], package["manifest_path"])
')
    mozjs_manifest=$(sed -n 's/^mozjs //p' <<<"$registries")
    mozjs_sys_manifest=$(sed -n 's/^mozjs_sys //p' <<<"$registries")
    [[ -n "$mozjs_manifest" && -n "$mozjs_sys_manifest" ]] || {
        echo "could not locate the mozjs and mozjs_sys crate sources" >&2
        return 1
    }
}

run_scope() {
    locate_mozjs || return 1
    python3 tests/unit/engine_contract/tools/scope_manifest.py \
        --servo-dir . \
        --manifest tests/unit/engine_contract/api-manifest.toml \
        --scope tests/unit/engine_contract/api-scope.toml \
        --mozjs-root "$(dirname "$mozjs_manifest")/src" \
        --mozjs-sys-root "$(dirname "$mozjs_sys_manifest")/src"
}

run_scope_check() {
    locate_mozjs || return 1
    python3 tests/unit/engine_contract/tools/scope_manifest.py \
        --servo-dir . \
        --manifest tests/unit/engine_contract/api-manifest.toml \
        --scope tests/unit/engine_contract/api-scope.toml \
        --mozjs-root "$(dirname "$mozjs_manifest")/src" \
        --mozjs-sys-root "$(dirname "$mozjs_sys_manifest")/src" \
        --check
}

command="${1:-all}"
if [[ $# -gt 0 ]]; then
    shift
fi

case "$command" in
    sm) run_sm "$@" ;;
    qjs) run_qjs "$@" ;;
    diff) run_diff ;;
    generate) run_generate ;;
    manifest) run_manifest ;;
    coverage) run_coverage ;;
    scope) run_scope ;;
    scope-check) run_scope_check ;;
    all)
        run_sm
        run_qjs
        run_diff
        run_coverage
        ;;
    *)
        echo "usage: $0 {sm|qjs|diff|generate|manifest|coverage|scope|scope-check|all} [cargo test args...]" >&2
        exit 2
        ;;
esac
