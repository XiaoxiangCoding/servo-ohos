#!/usr/bin/env bash
# Run the engine contract suite against SpiderMonkey (the reference) or QuickJS, then compare.
#
# Usage:
#   tools/run.sh sm       [--case ID]... [--cases ID[,ID...]]... [cargo test args...]
#   tools/run.sh qjs      [--case ID]... [--cases ID[,ID...]]... [cargo test args...]
#   tools/run.sh diff
#   tools/run.sh generate   regenerate api-manifest.toml from the current mozjs bindgen and shim
#   tools/run.sh manifest   check api-manifest.toml against the current mozjs bindgen and shim
#   tools/run.sh coverage   check the coverage of api-manifest.toml in sm.jsonl
#   tools/run.sh scope      regenerate api-scope.toml from Servo's actual usage
#   tools/run.sh scope-check  verify api-scope.toml is fresh
#   tools/run.sh report     compare HEAD's api-manifest.toml with the working tree
#                           [--baseline REV:PATH|FILE] [--manifest FILE] [--output FILE] [--format toml|tsv]
#   tools/run.sh fix-check  verify removed differences have fix-record.md entries
#   tools/run.sh all       [--case ID]... [--cases ID[,ID...]]...
#
# Environment:
#   QJS_DIR         local servo-quickjs checkout (default: ../servo-quickjs)
#   OUT_DIR         output directory (default: target/contract)
#   CONTRACT_CASES  comma/space separated case ids; normally set by --case/--cases
#   CONTRACT_CASES_OUT  runtime case inventory written by the test harness
#   FIX_CHECK_BASELINE  baseline known-differences.toml for fix-record checks (default: HEAD)

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../../.." && pwd)"
out_dir="${OUT_DIR:-$repo_root/target/contract}"
qjs_dir="${QJS_DIR:-$repo_root/../servo-quickjs}"

lockfile="$repo_root/Cargo.lock"
lockfile_backup=""

restore_lockfile() {
    if [[ -n "$lockfile_backup" && -f "$lockfile_backup" ]]; then
        mv "$lockfile_backup" "$lockfile"
        lockfile_backup=""
    fi
}

trap restore_lockfile EXIT

# Cargo rewrites Cargo.lock when a local path patch replaces mozjs. Keep that build side effect
# out of the working tree, but never overwrite a caller's own lockfile change.
protect_lockfile() {
    [[ -f "$lockfile" ]] || return 0
    if [[ -n "$(git -C "$repo_root" status --porcelain -- Cargo.lock)" ]]; then
        echo "Cargo.lock has uncommitted changes; commit or restore it before running the QuickJS suite" >&2
        return 1
    fi
    lockfile_backup=$(mktemp "${TMPDIR:-/tmp}/engine-contract-Cargo.lock.XXXXXX")
    cp "$lockfile" "$lockfile_backup"
}

# Resolve a caller-supplied relative OUT_DIR against the invoking directory, not the repository
# root or the crate directory that `cargo test` runs from. `sm`, `qjs` and `diff` must agree on it.
case "$out_dir" in
    /*) ;;
    *) out_dir="$(pwd)/$out_dir" ;;
esac

parse_contract_args() {
    local -a caller_args=("$@")
    local -a parsed_cases=()
    local -a parsed_cargo=()
    PARSED_CONTRACT_CASES=()
    while [[ ${#caller_args[@]} -gt 0 ]]; do
        case "${caller_args[0]}" in
            --case)
                [[ ${#caller_args[@]} -ge 2 && -n "${caller_args[1]}" ]] || {
                    echo "--case requires a case id" >&2
                    return 2
                }
                parsed_cases+=("${caller_args[1]}")
                caller_args=("${caller_args[@]:2}")
                ;;
            --case=*)
                [[ -n "${caller_args[0]#--case=}" ]] || {
                    echo "--case requires a case id" >&2
                    return 2
                }
                parsed_cases+=("${caller_args[0]#--case=}")
                caller_args=("${caller_args[@]:1}")
                ;;
            --cases)
                [[ ${#caller_args[@]} -ge 2 && -n "${caller_args[1]}" ]] || {
                    echo "--cases requires one or more case ids" >&2
                    return 2
                }
                parsed_cases+=("${caller_args[1]}")
                caller_args=("${caller_args[@]:2}")
                ;;
            --cases=*)
                [[ -n "${caller_args[0]#--cases=}" ]] || {
                    echo "--cases requires one or more case ids" >&2
                    return 2
                }
                parsed_cases+=("${caller_args[0]#--cases=}")
                caller_args=("${caller_args[@]:1}")
                ;;
            --)
                caller_args=("${caller_args[@]:1}")
                parsed_cargo+=("${caller_args[@]}")
                break
                ;;
            *)
                parsed_cargo+=("${caller_args[0]}")
                caller_args=("${caller_args[@]:1}")
                ;;
        esac
    done

    if [[ ${#parsed_cases[@]} -gt 0 ]]; then
        local joined
        joined=$(IFS=,; echo "${parsed_cases[*]}")
        CASE_SELECTION="${joined//[[:space:]]/,}"
        while [[ "$CASE_SELECTION" == *,,* ]]; do
            CASE_SELECTION="${CASE_SELECTION//,,/,}"
        done
        CASE_SELECTION="${CASE_SELECTION#,}"
        CASE_SELECTION="${CASE_SELECTION%,}"
        [[ -n "$CASE_SELECTION" ]] || {
            echo "--case/--cases did not name any case" >&2
            return 2
        }
    else
        CASE_SELECTION="${CONTRACT_CASES:-}"
    fi

    PARSED_CARGO_ARGS=("${parsed_cargo[@]}")
}

sm_case_inventory_path() {
    echo "$out_dir/sm-cases.json"
}

qjs_case_inventory_path() {
    echo "$out_dir/qjs-cases.json"
}

with_case_selection() {
    local selection="$1"
    shift
    if [[ -n "$selection" ]]; then
        CONTRACT_CASES="$selection" "$@"
    else
        env -u CONTRACT_CASES "$@"
    fi
}

run_sm() {
    local -a cargo_args=()
    parse_contract_args "$@" || return
    cargo_args=("${PARSED_CARGO_ARGS[@]}")
    local selection="$CASE_SELECTION"
    mkdir -p "$out_dir"
    cd "$repo_root"
    rm -f "$out_dir/sm.jsonl" "$(sm_case_inventory_path)"
    with_case_selection "$selection" env \
        CONTRACT_ENGINE=spidermonkey \
        CONTRACT_OUT="$out_dir/sm.jsonl" \
        CONTRACT_CASES_OUT="$(sm_case_inventory_path)" \
        cargo test --locked --offline --release -p engine_contract_tests "${cargo_args[@]}"
}

run_qjs() {
    local -a cargo_args=()
    parse_contract_args "$@" || return
    cargo_args=("${PARSED_CARGO_ARGS[@]}")
    local selection="$CASE_SELECTION"
    protect_lockfile || return
    mkdir -p "$out_dir"
    cd "$repo_root"
    rm -f "$out_dir/qjs.jsonl" "$(qjs_case_inventory_path)"
    with_case_selection "$selection" env \
        CONTRACT_ENGINE=quickjs \
        CONTRACT_OUT="$out_dir/qjs.jsonl" \
        CONTRACT_CASES_OUT="$(qjs_case_inventory_path)" \
        cargo test --offline --release -p engine_contract_tests --features js-quickjs \
        --config "patch.crates-io.mozjs.path=\"$qjs_dir/mozjs\"" "${cargo_args[@]}"
    local status=$?
    restore_lockfile
    return "$status"
}

run_diff() {
    [[ -f "$out_dir/sm.jsonl" && -f "$out_dir/qjs.jsonl" ]] || {
        echo "missing $out_dir/sm.jsonl or $out_dir/qjs.jsonl; run '$0 sm' and '$0 qjs' first" >&2
        return 1
    }
    local sm_inventory qjs_inventory
    sm_inventory=$(sm_case_inventory_path)
    qjs_inventory=$(qjs_case_inventory_path)
    [[ -f "$sm_inventory" && -f "$qjs_inventory" ]] || {
        echo "missing $sm_inventory or $qjs_inventory; run '$0 sm' and '$0 qjs' first" >&2
        return 1
    }
    echo "using runtime case inventories: $sm_inventory, $qjs_inventory"
    cd "$repo_root"
    python3 tests/unit/engine_contract/tools/compare.py \
        "$out_dir/sm.jsonl" "$out_dir/qjs.jsonl" \
        tests/unit/engine_contract/known-differences.toml \
        --sm-cases "$sm_inventory" \
        --qjs-cases "$qjs_inventory"
}

run_fix_check_after_diff() {
    local inventory
    inventory=$(sm_case_inventory_path)
    if [[ -f "$inventory" ]]; then
        run_fix_check --cases "$inventory" ${FIX_CHECK_BASELINE:+--baseline "$FIX_CHECK_BASELINE"}
    else
        run_fix_check ${FIX_CHECK_BASELINE:+--baseline "$FIX_CHECK_BASELINE"}
    fi
}

run_all() {
    parse_contract_args "$@" || return
    local selection="$CASE_SELECTION"
    local -a step_args=()
    if [[ -n "$selection" ]]; then
        step_args=(--cases "$selection")
    fi
    local -a failures=()
    local step
    for step in sm qjs diff fix-check coverage; do
        if [[ "$step" == "coverage" && -n "$selection" ]]; then
            echo "==> coverage (skipped: coverage requires the full case set)"
            continue
        fi
        if [[ "$step" == "fix-check" && ! -f "$(sm_case_inventory_path)" ]]; then
            echo "==> fix-check (no runtime inventory; checking records only)"
        fi
        echo "==> $step"
        if [[ "$step" == "fix-check" ]]; then
            if ! run_fix_check_after_diff; then
                failures+=("$step")
            fi
            continue
        fi
        if ! "run_$step" "${step_args[@]}"; then
            failures+=("$step")
        fi
    done
    if [[ ${#failures[@]} -gt 0 ]]; then
        echo "engine contract steps failed: ${failures[*]}" >&2
        return 1
    fi
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

run_report() {
    local baseline="HEAD:tests/unit/engine_contract/api-manifest.toml"
    local manifest="$repo_root/tests/unit/engine_contract/api-manifest.toml"
    local output=""
    local format="toml"
    local invoke_dir="$PWD"
    local baseline_path temporary_baseline=""
    local status

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --baseline)
                [[ $# -ge 2 && -n "$2" ]] || {
                    echo "--baseline requires REV:PATH or a file" >&2
                    return 2
                }
                baseline="$2"
                shift 2
                ;;
            --baseline=*)
                [[ -n "${1#--baseline=}" ]] || {
                    echo "--baseline requires REV:PATH or a file" >&2
                    return 2
                }
                baseline="${1#--baseline=}"
                shift
                ;;
            --manifest)
                [[ $# -ge 2 && -n "$2" ]] || {
                    echo "--manifest requires a file" >&2
                    return 2
                }
                manifest="$2"
                shift 2
                ;;
            --manifest=*)
                [[ -n "${1#--manifest=}" ]] || {
                    echo "--manifest requires a file" >&2
                    return 2
                }
                manifest="${1#--manifest=}"
                shift
                ;;
            --output)
                [[ $# -ge 2 && -n "$2" ]] || {
                    echo "--output requires a file" >&2
                    return 2
                }
                output="$2"
                shift 2
                ;;
            --output=*)
                [[ -n "${1#--output=}" ]] || {
                    echo "--output requires a file" >&2
                    return 2
                }
                output="${1#--output=}"
                shift
                ;;
            --format)
                [[ $# -ge 2 && -n "$2" ]] || {
                    echo "--format requires toml or tsv" >&2
                    return 2
                }
                format="$2"
                shift 2
                ;;
            --format=*)
                [[ -n "${1#--format=}" ]] || {
                    echo "--format requires toml or tsv" >&2
                    return 2
                }
                format="${1#--format=}"
                shift
                ;;
            *)
                echo "report: unknown argument: $1" >&2
                return 2
                ;;
        esac
    done

    case "$format" in
        toml|tsv) ;;
        *)
            echo "--format must be toml or tsv" >&2
            return 2
            ;;
    esac

    if [[ -z "$output" ]]; then
        output="$out_dir/api-changes.$format"
    fi

    cd "$repo_root"
    if [[ "$baseline" == *:* ]]; then
        temporary_baseline=$(mktemp "${TMPDIR:-/tmp}/engine-contract-baseline.XXXXXX")
        if ! git show "$baseline" >"$temporary_baseline"; then
            echo "could not read baseline '$baseline'" >&2
            rm -f "$temporary_baseline"
            return 1
        fi
        baseline_path="$temporary_baseline"
    else
        baseline_path="$baseline"
        [[ "$baseline_path" == /* ]] || baseline_path="$invoke_dir/$baseline_path"
        [[ -f "$baseline_path" ]] || {
            echo "missing baseline manifest: $baseline_path" >&2
            return 1
        }
    fi

    [[ "$manifest" == /* ]] || manifest="$invoke_dir/$manifest"
    [[ "$output" == /* ]] || output="$invoke_dir/$output"
    mkdir -p "$(dirname "$output")"
    if python3 tests/unit/engine_contract/tools/api_inventory.py report \
        --baseline "$baseline_path" \
        --manifest "$manifest" \
        --output "$output" \
        --format "$format"; then
        status=0
    else
        status=$?
    fi
    rm -f "$temporary_baseline"
    return "$status"
}

run_fix_check() {
    local baseline="HEAD:tests/unit/engine_contract/known-differences.toml"
    local cases=""
    local invoke_dir="$PWD"
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --baseline)
                [[ $# -ge 2 && -n "$2" ]] || {
                    echo "--baseline requires REV:PATH or a file" >&2
                    return 2
                }
                baseline="$2"
                shift 2
                ;;
            --baseline=*)
                [[ -n "${1#--baseline=}" ]] || {
                    echo "--baseline requires REV:PATH or a file" >&2
                    return 2
                }
                baseline="${1#--baseline=}"
                shift
                ;;
            --cases)
                [[ $# -ge 2 && -n "$2" ]] || {
                    echo "--cases requires a file" >&2
                    return 2
                }
                cases="$2"
                shift 2
                ;;
            --cases=*)
                [[ -n "${1#--cases=}" ]] || {
                    echo "--cases requires a file" >&2
                    return 2
                }
                cases="${1#--cases=}"
                shift
                ;;
            *)
                echo "fix-check: unknown argument: $1" >&2
                return 2
                ;;
        esac
    done
    local -a case_args=()
    if [[ -n "$cases" ]]; then
        [[ "$cases" == /* ]] || cases="$invoke_dir/$cases"
        case_args=(--cases "$cases")
    elif [[ -f "$(sm_case_inventory_path)" ]]; then
        case_args=(--cases "$(sm_case_inventory_path)")
    fi
    cd "$repo_root"
    python3 tests/unit/engine_contract/tools/fix_record.py check \
        --baseline "$baseline" \
        --differences tests/unit/engine_contract/known-differences.toml \
        --record tests/unit/engine_contract/fix-record.md \
        "${case_args[@]}"
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
    report) run_report "$@" ;;
    fix-check) run_fix_check "$@" ;;
    all)
        run_all "$@"
        ;;
    *)
        echo "usage: $0 {sm|qjs|diff|generate|manifest|coverage|scope|scope-check|report|fix-check|all} [--case ID|--cases ID[,ID...]] [cargo test args...]" >&2
        exit 2
        ;;
esac
