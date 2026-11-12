# ALIA-P01 acceptance evidence, version 1

The existing `scripts/test_live_lifecycle.py` remains the only orchestration entry. It runs `first-study → restore → observe → unknown` using the existing Rust examples, one disposable database and the same persisted individual. Existing isolated modes remain available. This package adds no lifecycle implementation, manual goal completion, accelerated scheduling, provider calls, runtime prompt/schema changes or deployment.

## Frozen criteria and independent review

`cases-v1.json` freezes eight criteria before the new joint trial. Its SHA256 is recorded in `baseline-review-v1.json` and the independent summary. Future changed criteria require a new version; do not edit a frozen version to accommodate results. These are development regressions, not held-out M5 statistics.

A reviewer compares actual claims with each target's declared sources and actual effects. A factual aside, pronoun or parenthetical assertion has the same evidence obligation as the main answer. Import metadata is not reading; citations on another target are not authorization; an atomic memory/goal write is distinct from external execution and research completion. A rejected draft is not a published experience. Correct known recall is checked separately from unknown refusal.

`check_results.py` validates SHA256-bound JSON sources and RFC6901 evidence pointers. It requires written claim/source comparisons for pass/fail, fills missing criteria with unverified, and leaves invalid evidence unverified. It does **not** perform automatic semantic scoring, keyword matching or provider-supported voting. A review is model-assisted independent comparison, not human certification. Optional `case_scope` and `case_checks` bind a narrower case group to actual report IDs; `case_behavior_pass` is separate from the complete eight-criterion `behavior_pass`. Missing case reviews stay unverified. Distinct author error, reviewer protocol error, semantic veto, runtime rejection and driver fault can coexist; the semantic author failure is determined by independent comparison, not inferred merely from a runtime error.

## Offline validation

From `/workapps/project-alia/alia`:

```sh
.venv/bin/python -m unittest discover -s evals/p01 -p 'test_*.py'
.venv/bin/python scripts/check_phase0.py
.venv/bin/python evals/p01/check_results.py evals/p01/baseline-review-v1.json --output evals/results/alia-p01-baseline-FRESH.json
```

All unit-test subprocesses are injected fixtures. They never invoke psql, Docker, a provider or a Rust binary. Exit codes: checker 2 means invalid review/provenance, 1 means evidenced behavioral failure, 0 means a valid review (inspect `behavior_pass`, because null is unverified). An exit 0 alone never proves behavior passed. Existing output files are refused.

## Coordinator checkpoint interface

The coordinator builds/copies binaries and provides a JSON manifest shaped as follows, using actual hashes and an identifier:

```json
{
  "id": "root-named-runtime-checkpoint",
  "sources": {"src/runtime/worker.rs": "64 lowercase hex SHA256 characters"},
  "binaries": {"live_individual": "64 lowercase hex SHA256 characters", "live_autonomy": "64 lowercase hex SHA256 characters"}
}
```

The root's equivalent manifest layout `format` + `files` + `binaries` is also accepted; the manifest's SHA256 uniquely binds that version. The manifest's source map must cover the actual runtime/config/schema/model-policy build snapshot, not merely the active checkout at driver start. Arbitrary additional checkpoint metadata can include resolved model identifiers, tokenizer profile/hash, build command and feature flags. No credentials, raw prompts, supplier response logs or private data belong in it. The flat binary-only historical manifest is insufficient to establish a complete code/config checkpoint; preserve it as historical evidence.

The driver defaults to `target/debug/examples` for compatibility. For a root-released real job use the frozen directory and manifest:

```sh
flock -x /tmp/alia-project-heavy.lock .venv/bin/python scripts/test_live_lifecycle.py --credentials .tmp.env --binary-dir /ABSOLUTE/ROOT/FROZEN/bin --checkpoint /ABSOLUTE/ROOT/FROZEN/checkpoint.json --output evals/results/ROOT-RELEASED-FRESH-JOB.json
```

This is a command template, not permission to make a paid call. Only the coordinator releases a named paid job. Do not rebuild binaries in P01 or repeat failed paid trials to seek a pass. Unit tests and M0 do not require the heavy resource lock. DB suites remain coordinator queued with `RUST_TEST_THREADS=1`.

## Explicit snapshot/artifact/behavior-DB mode

The root-approved interface also accepts a checkpoint **directory** containing immutable `source/` and `manifest.json` (`alia-p03-checkpoint/1`). Provide both `--binary-dir /ABSOLUTE/artifacts/bin` and `--test-db-state /PRIVATE/behavior.json`; the artifacts root must contain `artifacts.json` (`alia-p03-artifacts/1`). The legacy manifest-file and examples-directory invocation remains supported.

```sh
.venv/bin/python scripts/test_live_lifecycle.py --credentials /PRIVATE/provider.env --checkpoint /ABSOLUTE/checkpoint --binary-dir /ABSOLUTE/artifacts/bin --test-db-state /PRIVATE/behavior.json --output /ABSOLUTE/FRESH-lifecycle-report.json
```

This remains a command template, not a paid-job release. Explicit mode loads the P03 QA adapters from SHA256-bound snapshot bytes, disables bytecode writes to the snapshot, verifies the complete source/artifact set through `artifacts.verify_artifacts`, and runs every example and SQL subprocess with the immutable snapshot source as cwd. It checks that the executing driver itself matches the checkpoint. It preserves actual `build_commands`, `build_environment`, manifest hashes and public database identity in provenance. Missing inputs or a mismatched/unlisted artifact fail before trial startup; no `.env`, default Compose, changing checkout helper or build fallback is allowed.

Only an explicitly ready `behavior` resource with exact owned container/volume identity is accepted. Resource ownership is checked before each subprocess. The parent P03 runner owns the container and volume; P01 creates and drops only its own random `alia_live_test_*` database. A replaced resource blocks access and cleanup rather than targeting an untrusted replacement. P03's actual heavy-lock adapter holds or verifies the live descriptor continuously through setup, stage execution, final accounting, database cleanup and final source/artifact verification. Final verification failures preserve the fee and cleanup report.

P01 unit tests inject the orchestration interfaces and subprocesses; they prove driver behavior and no-fallback paths. They do not certify P03's implementation or real integration. P03 separately owns its helper tests, actual two-database isolation, immutable build and full engineering report. Final integration and the full real lifecycle remain unverified until run on the root-released artifacts/database.

## Failure and fee semantics

The driver reserves a fresh report before preflight. Each attempted stage records exit code, parse errors, engineering checks and real elapsed time. Complete JSON objects survive adjacent malformed/incomplete lines; the stage still fails. Raw malformed output and stderr are withheld. Timeout, no JSON, invalid JSON, duplicate keys, multiple reports, missing fields and spawn failure have fixed diagnostics. Unattempted phases are explicit.

The same bounded final ledger is collected before cleanup on both success and failure. It records reservation IDs/run IDs/status/actual tokens/charged tokens/unknown flag, full count and 256-row limit. A truncated, unknown or dispatched ledger never becomes zero or an exact final total. `observed_ledger_charged_tokens` is only the visible subtotal; `charged_tokens=null` means final known total unavailable. Currency cost remains unknown. Cumulative stage snapshots are never summed; historical last snapshots are observations with completeness unverified. Cleanup has its own result and never erases the primary failure or known fees.

`engine_pass` requires complete phases, final known accounting and successful cleanup. `behavior_pass` remains null in the raw driver output until independent review. Engineering failure, safe runtime containment and successful behavior are different results. No report certifies full M1/M2, growth, deletion, fourteen-day observation or release.

## Baseline

`baseline-review-v1.json` binds five old reports by SHA256 and contains the first independently checked baseline. `evals/results/alia-p01-baseline-v1.json` is a fresh summary; no old report was overwritten. The baseline fails unknown-history and unsupported-person/citation criteria; atomic-commit coverage and final accounting completeness remain unverified. Historical reports lack a full binary/source/config checkpoint, so they do not establish the current runtime version. A subsequent root-fixed trial gets a separate report and review, preserving this baseline.

## Structured defect and evidence-gap handoff

`diagnostics.py` consumes a validated independent review summary, rechecks the source-report hashes and exact evidence pointers, and publishes a fresh JSON packet. Each entry carries the expected criterion, actual independent comparison, candidate/report version, observed model identifiers and routing owner/module. Historical code/config versions that were not pinned remain unknown. Failed comparisons, evidence gaps, and observed runtime/protocol events are different kinds; a recovered review schema error is preserved without converting it into semantic failure. Missing/unexecuted coverage is not fabricated into a production defect.

```sh
.venv/bin/python evals/p01/diagnostics.py evals/results/p01-segments-uncertainty-v1.independent.json --output evals/results/FRESH-diagnostics.json
```

Routing is an investigation entry, not a proved root cause, production-edit authorization or permission to make another paid call. The sole coordinator can reroute it after inspecting the exact evidence. This offline helper and its tests were added after N2 froze; they do not change or certify N2's runtime, criteria, driver or engineering result.
