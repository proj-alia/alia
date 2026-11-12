# ALIA-P05-03 — real relationships and linked communication

Status: READY — locally verified; root registration observed; current-code full integration pending. Prior P05-02 complete and accepted; new bounded Goal active in same existing P05 task.0 real models, no deployment or full matrix.

## Final READY — P05-03

Replaced the global loaded-message relationship view with persisted relationship directory, selected real subject, saved stage/expectations/boundaries/commitments/unresolved and actually linked communication. Initial persona relationship seed is separately labeled; no guessed owner relation, made-up intimacy or fabricated history. Timeline exposes received/sent text and actual source/actor/event metadata only; drafts, unrelated messages and private provider/review/runtime payload are excluded. Arrays/objects render as complete readable escaped lists/fields. UUID subject is displayed separately from Alia identity. Existing Web exchange link/legacy relation route work.

Backend:6 actual HTTP/PostgreSQL cases PASS,0 ignored. Includes real HTTP initialization + owner message path, >30 directory/timeline, ascending subject / descending sequence, independent records, concurrent append, current visibility and still-visible cursor anchor, namespace/subject/sequence tampering, strict params/auth/no-store, unknown identity-as-subject404, DB ownership FK rejection, multi-generation/cyclic source blocking and complete read-side-effect digests. Each GET uses one REPEATABLE READ READ ONLY transaction. Strict Clippy -D warnings and Rust binary build exit0. No schema/migration/production execution rules changed.

Final browser:9 groups PASS with actual Chromium/HTTP/PostgreSQL and explicit synthetic records (no mocked routes). Real table-lock delayed A→B switching and multi-generation revocation/SSE do not restore old private text. Actual ALTER TABLE fault clears timeline and retry restores it. Offline/reconnect/logout clear personal views, storage stays empty; no usage/tool-result fake reads. Keyboard, dark and360/390/1440 tested.390px/dark final screenshots visually inspected and readable. Parent lock held through service/container/volume cleanup; all cleanup PASS, own sources unchanged. v1 passed entire local pipeline; v2 only repeated build/browser after UI rendering refinement. Both original reports retained, no unexpected failure hidden.

Actual M0 exit0:5 schemas/49 closed objects/9 examples/52 negatives/111 traceability/136 task edges/9 docs/34 links/2 sources. Own rustfmt --check and node --check exit0.

Scope: verified immutable N3 assembly (261c7921fe297c798bb2f4a0bb5b604cdee6f8e38b957393f3d79fb75956f487) + only eight own source overlays and minimal derived module/GET registrations. Does not certify ROOT-BELIEF-03 or other current core changes. Root live3 GET/module registration separately observed. No P05-01/P05-02 accepted package edits; every accepted hash intact and all live P05-02 nine files unchanged. No Settings/Notes/Documents/App/shared hook/core/authority-doc changes.0 real model calls/0 deployment; no full-matrix or relationship behavior/AT-08 completion claim.

Evidence: `evals/results/p05-relationships-summary-v1.json`; backend+Clippy `p05-relationships-local-v1/report.json`; final browser `p05-relationships-local-v2/browser/report.json`; final parent `p05-relationships-local-v2/report.json`. Reproduction parent saved as `evals/results/p05-relationships-local-runner.py`: `python3 evals/results/p05-relationships-local-runner.py --output /absolute/new-output-directory` (requires existing local toolchain/node_modules/Docker privileges; no .env or provider credentials read). Uses explicit owned ephemeral service/PG/binary/web and same global heavy lock, serial DB. Default runs only own6 HTTP/PG + strict Clippy/build/browser, not full matrix.

Next dependency: main verifies listed SHA and accepts source slice into its next specifically released candidate; P03 performs current-code full engineering integration. Root registration is complete; no further P05 permission needed. Existing source packages and fixed N1/N2/N3 evidence remain immutable.

### Exact final source hashes

- `src/adapters/relationships_view.rs`: `9467c4ccd6434c8db2bf2cfd44043cd4690ede8498993c0c8bc89e6ec2e9f44e`
- `src/service/relationships_view.rs`: `60207ec1f1f11e6d41b30c1ca859a80b101a05bf3fc7eab698842d5384a23991`
- `tests/relationships_view.rs`: `6c0d7bdf66e509cc04574d9a43b3393c3fca0ace505ed5cf601790696982c4c3`
- `web/scripts/test-relationships.mjs`: `3d6c3885b7af8b949783c869f5c979850fe78beb5d636d7ab1f019242578da4e`
- `web/src/features/Relations.tsx`: `f59facc5d8720fd0c972ebe2a143e831003fafab28aac4fd47978b4f31d92300`
- `web/src/features/relationships/relationships.css`: `a5ad1930814325b09e74d2a61155b58d98bd5ace6522bec18a17a2db395b4aad`
- `web/src/features/relationships/types.ts`: `d248d73d9ea67881f3efc3ba296d0307ba56ece9ad9932a05d51ac3f402bfcc5`
- `web/src/features/relationships/useRelationshipsRead.ts`: `8a6227f6fa75896c3a1d1b0f6377fd2aecc058b6d8d59e1132c46f7765115c52`

### Evidence hashes

- Summary: `59a3b2a1797e383da86cdc6a62c78c3823dae2c0e712875ea35282260a64d6af`
- v1 parent: `2c9817a10cbdb901147c7207b0d8507d4ffac057bc7d955d273bbd0f36a19658`
- v2 parent: `61f0bd9ebc006c4f89e25f3ceefabee9547f0d6dd198ff8f7323e476e00c3388`
- v2 browser: `c64431cf24fcff92c85b7aa707559aef1332a64e704be87fc20fa17ae7160666`
- Fixed binary: `cc61bbe62ee9a6ac8f770389edea35e4a3de109d2943d5937138910bf9d96904`


## Baseline / boundaries

Authoritative frozen contract: ALIA_PARALLEL_COORDINATION.md P05-03. Relations.tsx baseline 9df3c33b962d645d36a1f0b68049ba377de55b295cb89a228d0d05a6beb23fc5 verified before editing. Own only new relationships_view adapter/service/test, features/relationships/**, test-relationships.mjs and Relations.tsx. No Settings/Notes/Documents/App/shared hooks/core/schema/migrations changes. P05-01/P05-02 accepted snapshots untouched. Global heavy lock retains parent through cleanup. Root remains sole interface/registration coordinator.

## Backend saved — exact root registration ready

`src/adapters/relationships_view.rs`: inherent Store::public_relationships(cursor:Option<&str>,page_limit:u32), public_relationship(subject:Uuid), public_relationship_events(subject:Uuid,cursor:Option<&str>,page_limit:u32), all async Result<serde_json::Value,StoreError>.

`src/service/relationships_view.rs`: public async handlers `relationships(State<Runtime>,Result<Query<PageQuery>,QueryRejection>)->Result<Json<Value>,ApiError>`, `relationship(State<Runtime>,Result<Path<Uuid>,PathRejection>,Result<Query<EmptyQuery>,QueryRejection>)->Result<Json<Value>,ApiError>`, `relationship_events(State<Runtime>,Result<Path<Uuid>,PathRejection>,Result<Query<PageQuery>,QueryRejection>)->Result<Json<Value>,ApiError>`. PageQuery accepts only cursor/limit; EmptyQuery accepts no fields.

Minimum root integration: add `mod relationships_view;` in adapters/service; within existing /api/v1 router add `.route("/relationships",get(relationships_view::relationships))`, `.route("/relationships/{id}",get(relationships_view::relationship))`, `.route("/relationships/{id}/events",get(relationships_view::relationship_events))`. Preserve middleware/auth/security.

Each response has one repeatable-read readonly tx, current identity/version/privacy metadata, recursive source and subject visibility. Directory ascending subject UUID; events only actually linked received/sent with string text descending actual sequence; cursors bind namespace/order/subject and still-visible anchor. Stored interpretations are not independently verified feelings or world facts. No mutations or generic-message-derived relations.

## Current verification / next dependency

Own Rust files rustfmt completed. No P05-03 compile/HTTP/PG/browser result yet. UI/tests underway; root may register saved handlers immediately. Local scoped validation proceeds without waiting for full engineering; P03 retains full integration. Failed evidence retained, not counted as pass.

Root live registration now observed: both relationships_view modules and all3 GETs are saved. Local tests use a temporary verified immutable N3 base + own8-file overlay + minimum derived registrations, no live core edits or changes to frozen packages. Backend actual HTTP/PG cases exercise production initialize/owner-message link and independent synthetic relationship records, including page/cursor/visibility/no-side-effects. Full current-code integration stays P03.

v1 local actual result:6 HTTP/PostgreSQL tests PASS,0 ignored; strict Clippy -D warnings, Rust build, tsc/Vite and9 Chromium groups all exit0. Parent/service/container/volume cleanup PASS and own sources unchanged. Browser actual slow-switch and two-generation source-revocation tests passed.390px/dark screenshots inspected. Small UI refinement now renders stored arrays/objects as readable lists/fields rather than JSON punctuation; values remain complete and escaped. v2 reruns browser/build for this rendering-only change; backend files unchanged and v1 DB/Clippy evidence retained. No failures in v1.
