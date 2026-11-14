# QuickJS shim fix record

This file is the A4 hand-off between "fix the shim and regress" and the next full QuickJS run.
`quickjs-difference-allowlist.toml` records only differences that are still present. When a shim
fix makes an entry disappear, add a record here before deleting the entry from the allowlist.

Required for every fix:

- one or more contract case ids that covered the defect;
- the QuickJS commit or change that fixed it;
- what changed in the shim, in one or two sentences;
- the command and result that proved the difference disappeared.

The regression command must run both engines with the same case selection and finish with
`run.sh compare-engine-results`. A fix is not closed until the comparison reports zero missing cases, zero unaccepted
differences, and no entry for the case being removed (or a `pending`/`bug` entry while follow-up
work remains).

## Template

Copy this block for each fix. Keep newest records first.

```markdown
## <case id>

- Date: YYYY-MM-DD
- Owner: <name or team>
- Issue: <link or "none">
- Previous status: <pending | bug | accepted | wontfix>
- Previous severity: <P0 | P1 | P2 | P3>
- Fixed in: <servo-quickjs commit sha or PR>
- Shim change: <what changed and why the contract now matches SpiderMonkey>
- Regression: `run.sh run-spidermonkey-contract --case <case id> && run.sh run-quickjs-contract --case <case id> && run.sh compare-engine-results`
- Result: `0 mismatches, 0 unaccepted, 0 missing`
- Follow-up: <none, or the remaining accepted difference and its case id>
```

## Records

_No fixes recorded yet._
