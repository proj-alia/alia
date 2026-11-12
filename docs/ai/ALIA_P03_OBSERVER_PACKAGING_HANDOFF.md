# ALIA-P03-06 — source-declared ninth observer READY

Task: ALIA-P03-06. Coordinator/root retains interface and next candidate ownership. This bounded implementation and free offline validation is READY; no new full engineering or actual-model trial is released/executed here. N5 remains failed, with contact::new_input_and_deleted_ancestors_cancel_stale_drafts Storage(StaleState); its report/source/artifacts are unchanged.

## Changes and contract

- `scripts/qa/artifacts.py`: fixed original service + eight example names remain required. Add `bin/examples/live_belief_revisions` only if the already verified checkpoint manifest has the exact source key `examples/live_belief_revisions.rs`. No arbitrary source/manifest/target binary name selects a runtime. Missing ninth or its digest fails; an undeclared hash-covered ninth/arbitrary extra under bin also fails.
- `scripts/qa/run_engineering.py`: only import/shared artifact-name selection changes. Publication copies the fixed names, includes the ninth in p01-manifest.json binaries and the complete artifact hash map when declared. Existing `cargo build --locked --bins --examples` already builds all examples in the fixed execution tree; no compiler flags, profiles, mandatory stages, budgets or lock lifecycle change. Other Run methods are AST-identical to N5.
- Checkpoint binding, exact artifact inventory, full per-file digest/path checks and all ancestor/leaf symlink refusals remain. Binary-only consumers also require the declared observer. Target files/live checkout/lookalike nested sources cannot opt a legacy checkpoint in. Old compatibility manifest remains digest-covered.
- `evals/p03/test_observer_artifacts.py`:20 byte-fixture tests use actual Run.publish_artifacts and verify_artifacts/freeze round trips; no Rust/compiler/browser/PG/provider execution. Covers legacy/new positives, preserved eight, missing build/file/digest, extra/undeclared/corrupt runtime or compatibility manifest, late live and nested source, arbitrary names, path traversal, leaf/parent symlink, corrupted frozen source and unlisted static file.

## Actual command results and failures

1. `PYTHONDONTWRITEBYTECODE=1 .venv/bin/python -m unittest discover -s evals/p03 -p test_observer_artifacts.py -v`: initial19 tests,18 passed/1 fixture error, exit1. Corruption injection attempted to write the correctly read-only published binary; PermissionError prevented the injection. Test now explicitly chmods only its own temporary fixture before corrupting it. No publication protection was relaxed; first log preserved.
2. `PYTHONDONTWRITEBYTECODE=1 .venv/bin/python -m unittest discover -s evals/p03 -p 'test_*.py' -v`:66 tests (old46 + new20), all passed, actual exit0. Includes existing isolation/cleanup/mandatory-management stages; fixture failure reports in stdout are expected negative tests, not a paid/live job.
3. `PYTHONDONTWRITEBYTECODE=1 .venv/bin/python scripts/check_phase0.py`: actual exit0,5 schemas/49 closed objects/10 examples/56 negatives. Python AST parse succeeded for all3 changed/new QA files.
4. New live validator verified immutable N1/N2/N3/N4/N5 complete source/artifact trees,13 artifacts each, without requiring the ninth for legacy sources. All original report hashes remain unchanged; N1/N5 remain failed. Evidence `evals/results/p03/p03-06-ready/legacy-compatibility.json`.

No actual new observer was compiled or trialed by this task. Fixtures prove packaging/failure handling, not observer behavior, evidence quality, actual model semantics or a full candidate pass. The new P01 observer/driver and final core fix still need immutable acceptance, root naming, and a separately released engineering run. No model/DB trial, paid call, full matrix, deployment, production initialization, old artifact mutation or lock split occurred. Source hashes below are the precise QA inputs for that later candidate.

## READY source SHA256

- `scripts/qa/artifacts.py`: `8b79f3dff9b60d54567a5ba59496659383aaa16488c5a2fea8d80f837ff754ad`
- `scripts/qa/run_engineering.py`: `93802afc2aa1fb3bfa578d1616df655b8460a7986b92580bc1b9a518772752cf`
- `evals/p03/test_observer_artifacts.py`: `cc54c42082626ac44f819c6235cd8b71b46c02c05bcd65c4a19d231682cbed61`
