# ALIA-P03-09 — isolated one-shot Python executor READY

Only new src/adapters/sandbox.rs, tests/sandbox.rs and this handoff. Root retains adapter/capability registration, authorization/admission, schemas/Cargo/shared policy and verified-artifact design. No production registration, model invocation, extra full matrix or deployment by P03. Tests use immutable N5R3 source plus the2 owned files; they include the adapter by path without editing adapters.rs. Final source and source-copy hashes match.

## Public interface and actual behavior

DockerCommand has fixed unit variants Native/SudoNoPassword using /usr/bin/docker and /usr/bin/sudo -n. No arbitrary command/path/shell strings. SandboxExecutor::new(image_digest:&str,command:DockerCommand)->Result<Self,SandboxError> accepts only exact python@sha256:<64 lowercase hex>. execute(&self,request:&SandboxRequest)->SandboxOutcome is synchronous; root later supplies spawn_blocking, admission/permissions and authoritative action receipts. SandboxRequest has only source:String and timeout_ms:u64, deny_unknown_fields; source1..32768 UTF8 bytes, timeout100..10000ms. Invalid input fails without a process; Debug shows byte count, not source.

Outcome fields: execution_id:String (the exact unique alia-sandbox-<uuid32> container name, retained for recovery), status:SandboxStatus serialized snake_case, exit_code:Option<i32>, stdout/stderr:String, output_truncated:bool, duration_ms:u64, image_digest:String, isolation_verified:bool, cleanup_passed:bool. Statuses are succeeded/code_failed/timed_out/resource_exceeded/output_limit/unavailable/infrastructure_failed. Real container exit/OOM evidence drives classification; exit137 alone is not invented OOM. Cleanup failure overrides success and preserves the handle. No arbitrary host artifact export.

Runtime uses a private0700 /tmp client directory with0600 empty Docker config, fixed local Unix socket, cleared host CLI environment and fixed absolute launcher. It never inherits caller credentials, DATABASE_URL, proxy or DOCKER_HOST and never reads .env/Compose/current service. Runtime image pulls are forbidden; explicit admin/test preparation is separate. User source is hex-encoded only inside bounded stdin payload, not argv/disk/log; Python executes -I -B -. Container environment is cleared before user code, retaining fixed PATH only.

Create→inspect actual full ID/unique name/owner+role/config/image identity→start attach. Before starting, verify networknone/no ports, rootfsRO, cap-dropALL/no-new-privileges, builtin defaultseccomp availability, uid/gid65534, pids64, memory/swap128MiB, nanoCPU1core, private IPC/cgroup/PID/UTS modes, no privileged/additional caps/groups/devices/binds/volumes, no image-declared volumes, no healthcheck, no log-driver disk growth. /workspace and /tmp are separate16MiB rw,nosuid,nodev,noexec tmpfs, mode1777, cwd/workspace. Image actualID and RepoDigest and exact image environment must match. Caller cannot choose these controls.

A fixed trusted stdin bootstrap runs BEFORE compile/exec of decoded user source. It checks actual kernel UID/GID, isolated Python flags, CapEff0/NoNewPrivs1/Seccomp2, private cgroup memory.max128MiB/memory.swap.max0/pids.max64/cpu.max1core, rootro, tmpfs actual flags/size/mode1777 and only loopback interface. Missing controls exit without user execution and return unavailable. A fresh first-output attestation is removed from public stdout; source cannot run before it. No policy downgrade or automatic execution retry.

## Hard bounds and cleanup

Nonblocking Unix sockets concurrently feed bounded stdin and read each output stream, no blocking reader/writer futures. Each host CLI is polled against a monotonic remaining deadline; user output max64KiB per stream, also bounded after lossy UTF8 conversion with truthful truncation. Timeout/output-limit stops the real container and launcher group. The fixed privileged kill helper itself is bounded; unconfirmed launcher reaping is tracked as incomplete cleanup, not success. Static infrastructure errors never return source, private environment or daemon error bodies.

Requested timeout is an upper bound on setup+execution+teardown, not a promise that all time is available to Python.1000ms is reserved for owned teardown; valid budgets≤1000ms conservatively return timed_out without creating a container. E.g. actual3000ms infinite-loop case ran user code (attested), was stopped, inspected and removed within budget. Cleanup re-inspects actual identity/ownership and removes only full actualID, then independently confirms absence. Lost/ambiguous setup, identity mismatch or failed cleanup is not silently considered clean; execution_id remains for controlled recovery. No background execution is reported successful when controls/cleanup cannot be proven.

## Actual evidence, commands and preserved failures

Final sandbox-targeted-v6 parentPID1936664/session97470 naturally exit0; global heavy lock held through setup/test processes/cleanup, queued47.745s. Commands actually exit0:

- cargo test --locked --test sandbox:7 offline guards pass,9 actual tests explicitly ignored (not counted passed).
- cargo test --locked --test sandbox -- --ignored --nocapture:9 actual tests pass,0fail/0ignored,7 filtered.13 evidence records:10 actual attested payload runs,1 created-never-started foreign ownership control,2 unavailable admissions. Source snapshots fixed; no .env/live core compilation or DB/model trial.
- cargo clippy --locked --test sandbox -- -D warnings; rustfmt --edition2024 --check src/adapters/sandbox.rs tests/sandbox.rs; .venv/bin/python scripts/check_phase0.py: all actual0. M0 on the fixed workspace:5schemas/49closed/10examples/56negatives.

Actual cases: pure computation/32768-byte input via stdin; genuine exit7/code_failed; parent secret/proxy/DOCKER_HOST not inherited, only PATH in container; rootfs denied and host private fixture/socket/QA paths unavailable; writable private tmpfs ENOSPC/noexec; real host TCP and synthetic DNS UDP positive controls before/after sandbox show namespace cannot reach those endpoints or an external route; actual PID fork rejection/cgroup event;128MiB OOM inspected with exit137/resource_exceeded; infinite loop and both-stream infinite output actually stopped, bounded64KiB evidence; invalid UTF8 conversion flagged output_limit; actual foreign-label container preserved by generic cleanup refusal then removed by its separately tracked test teardown. This QA user actually lacks Native Docker permission: explicit Native returns unavailable and never escalates to Sudo. Missing pinned image returns unavailable without pull or host-code fallback.

Independent final audit checked all69 known execution handles across v1—v6 absent, no private client directories/owned launcher processes, actual final source hashes and P03-08 READY unchanged. All final cleanup fields true. Prior v1 kept:7 actual failures due retained Command Stdio peers hiding EOF after engine CLI exited; fixed by dropping parent peer handles, no controls weakened. v2 kept:8 actual tests passed, strictClippy failed only collapsible_if in test DNS server; fixed. v3—v6 passing scoped refinements preserve reports and add UTF8 truthfulness, unavailable-host proof, actual1777 check and bounded kill-helper wait. Targeted development checks are not extra full candidate matrices or automatic user-code retries.

Official image preparation under the same global resource lock pulled python:3.13-slim-bookworm once; resolved pinned reference:

`python@sha256:a5a7017efc5969e00ae2b91c8eb0631049a569652aa4ffc4501769049ae45c72`

Runtime --pull=never; missing cache is unavailable. No model download, fee, production credentials/data, production resource or Docker socket inside the experiment.

## Sources, engineering mapping and limits

Primary Docker documentation used: [resource constraints](https://docs.docker.com/engine/containers/resource_constraints/) mapped to actual cgroup values/fork/OOM assertions; [default seccomp](https://docs.docker.com/engine/security/seccomp/) mapped to builtin-engine declaration plus actual Seccomp2/zero caps before source; [none network](https://docs.docker.com/engine/network/drivers/none/) mapped to loopback-only kernel view and independent host positive controls; [tmpfs](https://docs.docker.com/engine/storage/tmpfs/) mapped to actual flags/mode/16MiB ENOSPC/noexec, memory.swap.max0; [container create](https://docs.docker.com/reference/cli/docker/container/create/) mapped to pinned/no-pull create/inspect/start and strict actual ownership teardown.

Linux/Docker/cgroup-v2 adapter tested on this actual host; other host platforms are not validated. Containers share a host kernel, not a VM or proof against kernel vulnerabilities. Unsupported-engine controls were exercised with pure negative inputs; no host security configuration was disabled to manufacture a negative. Actual native permission and image-unavailable cases were tested. External daemon/kernel failures can leave cleanup unconfirmed: capability must stay unavailable and retain the handle, never claim successful completion. No complete M4, real model behavior, artifact export, controlled skill registration, production permissions or M5 acceptance follows from this bounded executor.

Later integration must prepare the pinned image explicitly and pass actual-test admission/global lock. The new ignored Docker tests must not be mistaken for SQLx coverage or run without this admission when root expands a full-suite collector. Root supplies the next verified-artifact/registration interface; do not invent or concurrently implement another policy here. Latest user route remains completeM5 engineering before concentrated actual-model acceptance,0 new paid trials meanwhile.

## Final READY source SHA256

- `src/adapters/sandbox.rs`: `a41c2b4f9bf139ecff9f7ede181de52eef30bda1eb42aa049995016e170a8ee7`
- `tests/sandbox.rs`: `94d06b1ab803321a7998076eb12edf0bd3b7ec96a530a7f05da8a25e9bf1946e`
