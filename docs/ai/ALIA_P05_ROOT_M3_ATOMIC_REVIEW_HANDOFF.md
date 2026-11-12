# P05 independent review — ROOT-M3-ATOMIC-01/r2

Latest **scoped_approved** for subject `4ce0d065424ac8bbb454e5d989d121779e7db472dfb0e57aedcc5aa8debce17d`, exact `root-m3-atomic-v6/source` (490 sources). Actual author delta to r1 is only the missing candidate→target strong FK and the unchanged P05 two-case test. Verified complete source/log manifests, seven sealed binary hashes, and byte-identical independent tests (`29268af66ec7af3772d811b7bf70f07ce0aa9879397b98036c991bd4233ab506`).

P05 independently executed the original4 PG plus unchanged two target-removed/target-blocked regressions on a newly isolated database: both commands exit0, 6 passed/0failed/0ignored. Cleanup and frozen source integrity passed; no compilation, model calls, production edits or full-matrix rerun. Evidence: [r2 independent report](../../evals/results/p05-root-m3-atomic-review-r2-v1/report.json). P1 F1 is resolved on this exact r2. This scoped approval covers schema29 atomic proposed-only behavior; it does not approve schema30 writer/runtime, M4, true growth, M5 or release. Root/P04 author source changing again requires renewed matching review. Original r1 RED/private experiment and preparation failures remain below, unaltered.

# P05 independent review — ROOT-M3-ATOMIC-01/r1

Status **changes_requested / P1 P05-M3-ATOMIC-F1**. Reviewer P05; author Root; task subject `1e79496e21f2153500cd2bdbd44bc29e4ce02aed0e150fec3ce9e853cd13944c`. Fixed `evals/results/root-m3-atomic-v4/source`, manifest08274e77fa9c75596df6affb16a8a84d93972dacd8adcf0bdf45c6919b0ca94b, original report312c0deb92dc6477beb68741131feb7cd58ce2ab6a6c87518919470de34144ac. Production files and own bridge/backup sources were not changed.

## Finding and actual reproduction

`src/adapters/development.rs:92` LINKS lacks `developments.id → developments.target_id`; `src/adapters/decisions.rs:89-94` cached receipt seeds the development IDs and trusts that strong graph. On a first model proposal a new target is allocated inside propose_in_tx and therefore was absent from original context.input_manifest. Neither the returned target ID nor that FK is seeded on receipt replay.

Two new independent actual PostgreSQL cases start a real synthetic model/reviewer step, atomically commit a new proposed expression candidate, delete general derivation edges, then separately (1) set actual target provenance_removed=true while its object remains unblocked, or (2) block only the actual target object. Both replay the original PreparedDecision. Both **actually return the cached receipt and candidate UUID**; required rejection fails. This is an opaque derived-identifier/source invalidation gap, not a claim of plaintext/credential disclosure. Fingerprint remains stable; first case also confirms no extra model calls (original2 remain).

Exact new test `evals/results/p05-root-m3-atomic-review-v1/source/tests/p05_m3_atomic_boundary.rs`. `cargo test --locked --test p05_m3_atomic_boundary p05_ -- --ignored --test-threads=1` actualexit101,0passed/2failed/0ignored. Logs explicitly show receipt_returned=true for both flags. Original Root four PG cases from exact sealed binary independently rerun4passed/0failed/0ignored; the original blocked-created-event test covers a different FK and stays unchanged. Original6 filtered tests in the new counterexample target were not counted passed.

Required correction: include the actual target FK in strong provenance or seed and validate actual target IDs in the Root cached receipt path. Keep removed/object-blocked checks and original creation-event/review/source checks. Since adapters/development.rs is P04's current upstream file, Root may coordinate a one-line LINKS completion with P04 or implement explicit target-reference extraction in its own decisions.rs; P05 does not edit either live owner file. The proposed one-line patch is `work/p05-peer-m3-atomic/target-fk-fix-proposal.patch`, not adopted/approved. Root's exact fixed version needs renewed P05 review.

## Scope checked and evidence

Verified all489 Root frozen input hashes, original report/evidence/log hashes and16 sealed executable hashes before tests. Read typed proposal checks, actual target catalog/current head/generation/scope assembly and commit recheck, per-target complete supporting+counterevidence review, ordinary trial_binding rejection, one outer transaction and proposed-only writer, cached output/review references and upstream FK graph. Existing tests independently confirm unchanged persona/seed/permissions, atomic rollback and same-intent retry, actual review rejection with settled synthetic charges retained. These positives do not cancel F1.

Independent buildexit0; original4PGexit0; new2PGexit101. New snapshot and original source unchanged, exact ownedPG container/volume cleanup passed and private temp empty. `evals/results/p05-root-m3-atomic-review-v1/report.json` and review-conclusion.json bind evidence/commands. 0 cloud model calls; synthetic provider total not measured. No full matrix, schema30 measurement writer/runtime evalrun, true growth/model/14day or production approval is implied.

A private proposed-fix experiment may follow in a separate immutable snapshot, preserving this RED and without changing Root/P04 live files. The conclusion remains changes_requested until the actual author fix is provided. Queued prior source/request-key reviews are not silently approved by this task.


## Private minimal-fix experiment completed

Separate immutable `evals/results/p05-root-m3-atomic-fix-experiment-v1` changes only strong LINKS in its private production copy by adding `UNION SELECT id,target_id FROM developments WHERE individual_id=$1`. Actual compileexit0 and all6PG (original4 + unchanged independent2) pass/0failed/0ignored. Original Root/source hashes unchanged, own experiment snapshot unchanged, precise ownedPGcleanup and private-temp empty true. Its source delta is explicitly marked private/not adopted; the original RED report remains0pass2fail and review remains **changes_requested**.

Private patch source before d37a50cccb03e4e568cd61cc5634045cf40df7501a9e0f1505865983faf4e33a → after ed37c8825d15537061de835f8e4569ef5865127ac21289bfbe47d288dfcf8eb7. Root/P04 can apply/rebase the supplied one-line patch or implement an equivalent actual-target reference check in Root-owned decisions.rs. This experiment is actionable evidence, not author acceptance or permission to overwrite P04's in-flight development.rs. Exact actual author revision still needs renewed independent review.

New parallel queue notice from P04: schema30 development trial programs/plans/cases/attempts/case_sources and `purge_development_trials_in(tx,individual,ids)` are being implemented and will be fixed for review later. This current review intentionally stays on Root's fixedschema29 atomic-only candidate. Shared/frozen P05-07 files and08/M4 modules are untouched. Schema30 deletion/export/backup/restore hook and catalog rebinding need a separate precise revision/frozen source; no inferred schema30 pass from this report. P04's new writer matrix and M4 canonical storage remain independent tasks, not prerequisites for Root's cached-target fix.
