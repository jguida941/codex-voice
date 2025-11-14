# Codex Latency Remediation Plan — 2025-11-13

## Summary
- The Phase 2B measurement gate recorded 10 real Ctrl+R → Codex round-trips (see `LATENCY_MEASUREMENTS.md`).
- Voice pipeline averages **1.19 s**, Codex averages **9.2 s** (~88 % of total latency, 10.5 s overall), so improvements to the voice path alone deliver <12 % latency reduction.
- Phase 2B implementation therefore remains blocked until Codex latency is reduced or stakeholders explicitly accept the trade-off.

## Goals
1. **Establish Codex latency baseline** broken down by backend path (persistent PTY vs CLI fallback) and by stage (spawn, first token, completion).
2. **Identify actionable remediation** to bring Codex latency below 2 s average (stretch: <1 s) so Phase 2 voice work has meaningful impact.
3. **Document go/no-go decision** before unblocking Phase 2B voice streaming work.

## Investigation Plan

### 1. Telemetry & Instrumentation
- Extend `timing|phase=codex_job|…` logging to include `first_token_ms`, `backend_type`, `pty_attempts`, `cli_fallback_used`, and error phases. (`BackendStats` already tracks these fields; expose them via logs.)
- Update `latency_measurement` to parse Codex logs so we can correlate pipeline latency with backend path (PTY vs CLI) and failure modes.
- Capture raw logs for each measurement run (short, medium, long prompts) and attach to `LATENCY_MEASUREMENTS.md` appendices for traceability.

### 2. PTY vs CLI Health Check
- Instrument how often the persistent PTY route succeeds vs falling back to CLI (`stats.cli_fallback_used`).
- When PTY fails, log reason (startup timeout, stale output, etc.) and the time spent before falling back; this determines whether we should disable PTY entirely or increase budgets.
- Add temporary flag to force CLI-only and compare latency; if CLI is consistently faster, simplify by skipping PTY.

### 3. CLI Execution Profiling
- Measure `codex` binary startup time separately from Codex API latency by wrapping the CLI invocation with `Instant::now()` for spawn, first stdout byte, and completion.
- Capture command-line arguments and environment differences between PTY vs CLI paths to ensure we are using the fastest mode (e.g., avoid `--session` setups that stall).
- Validate that the CLI is using the desired Codex region/profile; misconfiguration could route to slow endpoints.

### 4. Alternative Backends
- If CLI latency remains >5 s after tuning, evaluate:
  - **Persistent HTTP/WebSocket backend** talking directly to Codex API (bypassing CLI). Requires formal API contract but could reduce process spawn costs.
  - **Local caching or replay** for repetitive requests (probably insufficient if Codex compute dominates).
- Document pros/cons, required approvals, and security implications before implementation.

### 5. Success Criteria & Decision Gate
- Collect updated measurements after instrumentation. If Codex average drops below 2 s and voice remains ≥1 s, revisit Phase 2B; otherwise, prioritize Codex backend work.
- If Codex cannot be improved quickly (external dependency), stakeholders must explicitly accept that Phase 2B will only shave ~1 s off a 10 s flow before we resume voice work.

## Action Items
1. **Telemetry task:** Expose `BackendStats` fields in logs + measurement harness (owner: Codex backend team). Due before next measurement run.
2. **Measurement rerun:** Repeat 10 interactive samples once telemetry is live; update `LATENCY_MEASUREMENTS.md` with PTY/CLI attribution.
3. **Remediation decision:** Based on new data, choose between PTY tuning, CLI-only mode, HTTP backend, or deferring voice streaming work.
4. **Phase 2B gatekeeper:** Keep this plan referenced from `ARCHITECTURE.md`; Phase 2B coding may only resume after this document’s tasks are complete and stakeholders approve the path.

## References
- [`LATENCY_MEASUREMENTS.md`](LATENCY_MEASUREMENTS.md) — raw data proving Codex is the bottleneck.
- [`PHASE_2B_CORRECTED_DESIGN.md`](PHASE_2B_CORRECTED_DESIGN.md) — voice streaming design awaiting this gate.
- `rust_tui/src/bin/latency_measurement.rs` — measurement harness to extend with Codex telemetry.
