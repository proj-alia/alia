# ALIA-P03 frozen engineering regression

P03 owns infrastructure and evidence. P01 owns independent model behavior; P02
owns Console queries and its browser assertions. The main coordinator alone
selects candidates and makes product/release decisions. No paid model calls,
production deployment, production initialization or production data are involved.

All project commands below run with `/workapps/project-alia/alia` as explicit cwd.
QA uses stable flock resources under `/tmp/alia-project-locks`: full candidate,
target directory, database administration, individual database container,
browser output, sandbox output and workspace. A process may reuse only the exact
inherited lock inode requested by its operation. Full candidates remain mutually
exclusive. Unverified shared resources remain serialized; do not delete lock
files or infer ownership from file existence.

## Interfaces

- `scripts/qa/freeze.py --output <new checkpoint directory>` copies the explicit
  source allowlist, hashes before/after/copy, then atomically publishes
  `manifest.json`. Immutable `contexts/01.md` and `02.md` are provenance inputs
  for M0 only. No credentials, data, reports, dependencies, models or build outputs
  are included. A failed or successful checkpoint is never overwritten.
- `scripts/qa/freeze.py --output <checkpoint> --verify` validates the exact source
  file set and SHA256, rejects extra hidden files, unsafe paths and symlinks.
- `scripts/qa/run_engineering.py --snapshot <checkpoint> --output <new results>
  --python /workapps/project-alia/alia/.venv/bin/python
  --node-bin /tmp/alia-go-diagnostic/node-v24.11.1-linux-x64/bin
  --chromium /home/zero/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome`
  runs the matrix from a hash-identical execution copy. npm dependencies live in
  its own `web/node_modules` to preserve normal resolution; Rust targets, Vite
  outputs, frozen artifacts and reports are separate. Developer checkout and
  immutable checkpoint source are never build destinations.
- `scripts/qa/database.py start --role engineering|behavior` creates a pinned
  pgvector resource with independent owned volume, random password and dynamic
  loopback port. stdout contains only public identities and a private state path.
  `cleanup --state <private state>` validates exact ownership before removal.
  State requires directory0700/file0600, outside the repository. Do not copy it
  into evidence. Cleaned state revokes its stored password. Startup/cleanup
  failure retains diagnostic state and gets a failed report.
- `scripts/qa/check_isolation.py --output <new results>` validates actual two-DB
  data isolation, wrong-identity refusal, cleanup isolation and repeated cleanup.

`artifacts/artifacts.json` binds every service/example/static file to the source
manifest digest; exact file sets, required runtime coverage and all path components
are checked before browser use. `artifacts/p01-manifest.json` is a compatibility
projection for old P01 CLI. The registered shared contract is the checkpoint
directory plus `artifacts/bin` with `examples/`; P01 implements its own adapter.

Frozen SQLx entry requires `--checkpoint`, `--source-root`, `--test-db-state` and
`--target-dir`. Frozen browser entries additionally require `--binary-dir` and
`--web-dir`, plus a fresh `--output`. Explicit mode never loads `.env`, starts
Compose, uses development `target/debug` or silently rebuilds the checkout.
Legacy no-argument test entry behavior stays available for prior callers.

## Evidence and interpretation

The JSON/Markdown report records commands, actual exits, UTC start/end, queue and
run seconds, test summaries (including ignored/filtered), logs, config/source and
artifact hashes, resource ownership and cleanup. Mandatory unrun checks cannot
produce full engineering pass. Failure stays attached to that fixed candidate;
P03 does not modify production code to make a product defect disappear.

P01 report references are behavioral evidence only for their named frozen
version. Unknown usage is null/unknown, never zero. P03 makes zero paid calls.
Two-cycle main-development/test-chore/rework metrics preserve unknowns until
measured; there is no inferred productivity multiplier. Fourteen actual days,
M1–M5 gates, real model behavior and local CPU model quality are separate.

Offline fault tests:

```
.venv/bin/python -m unittest discover -s evals/p03 -p 'test_*.py' -v
```

They test fixture copying/mutation, malformed manifests, missing/unlisted/tampered
artifacts, parent symlink escapes, overwrite refusal, Docker ownership and partial
startup, cleanup failures, final-verification failure persistence, continuous
cleanup locking, credential redaction and unknown/ignored accounting. These unit
fixtures do not count as actual PostgreSQL or browser passes.

## Accepted management inputs and N4 matrix

The next QA version adds three **mandatory** stages, making 21 in total:
`management-inputs`, `overview-browser`, and `documents-browser`. The input gate
runs before compilation. Missing inputs, changed accepted source, a skipped
browser stage, child report/cleanup failure, or final integrity failure blocks
`full_engineering_pass`. Older frozen runners and reports retain their own matrix.

Original manifest bytes for accepted P02-02, P04-01 and P05-01 are stored under
`evals/p03/accepted-slices/` and checked against pinned SHA256. Every declared
accepted source, including Settings/Overview and `tests/memory_revisions.rs`, must
match the candidate manifest. Development checkout versions are never fallbacks.
The existing `cargo test --workspace --lib --tests -- --ignored` automatically
collects P04's HTTP/PostgreSQL module; the input gate rejects its absence.

`scripts/qa/management_browser.py --verify-inputs-only --checkpoint <checkpoint>
--source-root <execution copy>` performs that read-only input gate. For execution,
use the same helper with `--case overview|documents`, explicit `--binary-dir`,
`--web-dir`, `--test-db-state` and fresh `--output`. Each case requires the owned,
ready engineering PostgreSQL resource, creates a new random trial DB and service,
and injects only the case's P02/P05 environment contract. SQL argv has no password;
service and child receive no provider/deployment credentials. Only frozen service
and complete frozen static artifacts are used; no build or Compose fallback.

Parent holds the database-instance and browser-output locks through case setup,
all child exits, service shutdown, trial DB and private-runtime cleanup and final evidence.
Child owns browser cleanup. Lost CREATE replies still cause an exact generated
DB cleanup attempt. Each cleanup component is recorded independently, so service
failure cannot skip DB cleanup. Partial child failure/timeout, failed cleanup or
unverified final source/artifacts is a failed parent even if another subset passes.

Child output is first collected in an owned private temporary directory. Only
sanitized logs/JSON and safe fixture screenshots are exported. If credentials
appear in report text, screenshot export is withheld. Final child evidence must
bind the actual executable and reported source hashes and include all 9 overview
or 8 document check groups plus successful browser cleanup; exit0 alone is not a
pass. No model calls, production mutation or accepted child-source edits occur.

This QA integration is READY only after offline faults pass. It does not authorize
an immediate full run: root selects a new N4 assembly containing accepted slices
and these QA files, verifies its binding, then releases one complete matrix.
ROOT-BELIEF-02 development is outside that candidate unless separately accepted.
