# xezim: proposed upstream interactive-session contribution

[HDL evaluation](hdl-backend.md) · [Icarus integration](icarus-backend.md)

## Purpose and inspected revision

RGate needs a simulator that runs alongside an interactive schematic/source
editor. We currently use Icarus VPI for native HDL execution and keep xezim as
an experimental replay adapter. This document is a contribution proposal, **not
a statement that an upstream issue or PR has been submitted**.

Inspected xezim: 0.11.1, revision `b49defcd4fe2af0c4d0c9c17174980e2ebf813f5`;
paired core revision `2366c0d555d613a68e53295451938106d95b689f`. Recheck the
current upstream API before implementing. Repository:
<https://github.com/aionhw/xezim>.

## Existing strengths and the actual gap

The existing engine runs our Project F VGA/LFSR/PWM, UART loopback, and PicoRV32
instruction fixture. Four-state inspection, tri-state resolution, delays,
hierarchical reads, and wide values pass our small tests. This is not a general
conformance claim.

The public compiler simulator exposes `compile`, `simulate`, `get_signal`,
`set_signal`, time, and a run limit. However `simulate` combines event-loop
execution with register-init policy, simulation lifecycle callbacks, final-block
execution, and trace shutdown. Calling it repeatedly with larger limits is not a
supported interactive session. `set_signal` exists, but safe paused-boundary
injection and wakeup semantics need a supported contract, not manipulation of
private scheduler state.

RGate's adapter therefore recompiles/replays from zero with a timestamped input
journal. This is slow as runtime/history grows, repeats host side effects, and
cannot represent a genuine pause. Its 1 ns `step` is only a sampling quantum.
Replay is evidence that the language/value bridge works, not a solution to
interactive simulation.

## Proposed minimum public contract

Agree on names with maintainers; these are conceptual operations, not existing
APIs:

1. Build/elaborate/compile an owned session without running end-of-simulation
   work.
2. Initialize exactly once and settle time zero.
3. `advance_until(deadline, budget)` preserves all pending events and returns a
   stop reason.
4. `step_timestep(budget)` processes the next scheduled timestamp through the
   agreed settled/postponed boundary. Specify idle behavior and same-time
   pending work. Do not call this a single-event step.
5. Read typed four-state values via stable signal handles; enumerate names,
   widths, scope, kind, and directions.
6. Deposit an input at a defined writable boundary, resize/validate explicitly,
   dirty dependent logic, and settle through normal scheduler machinery.
   Distinguish deposit from force/release; do not accept arbitrary internal
   writes as input semantics by accident.
7. Subscribe to changes with defined timing (raw delta change versus settled
   value). Buffer records so callers need not execute reentrant callbacks during
   scheduling.
8. Explicitly finish once: final blocks, end-of-simulation callbacks, waveform
   flush/close. Define drop/abort separately from finish.

Expose integer engine ticks and time precision. RGate's current ns-only trait
loses sub-ns precision and should not dictate the upstream API. Return stop
reasons such as Deadline, Timestep, Quiescent, Finished, Stopped,
BudgetExceeded, Cancelled, and Error rather than overloading a work count.

## Implementation direction

Begin with an upstream design discussion and a temporary development fork. Keep
CLI behavior unchanged. Split one-shot execution into initialization, resumable
event-loop work, and finalization, retaining the one-shot method as a
convenience wrapper over those stages.

Review `src/compiler/simulator.rs` at the pinned revision:

- `compile`, `simulate`, and `init_registers`: one-time setup versus per-run
  lifecycle work.
- `event_loop_singlethread` deadline exit and next-time selection: pause without
  consuming future events or setting a terminal state.
- `run_events_until`: already services nested scheduler work, but is private and
  used under different assumptions. Do not merely make it public and assume
  lifecycle correctness.
- `get_signal`, `set_signal`, dirty flags, edge snapshots, and NBA queues: safe
  injection must wake consumers without duplicating or missing an edge.
- VPI/DPI active-simulator pointers, global settings, callbacks, and
  output/trace writers: restore context when paused, retain writers, and
  document concurrency restrictions.
- Profiling, parallel scheduling, nested task delays, pending
  inactive/NBA/assertion/postponed work: a deadline must not strand a partially
  executed time slot without explicitly defining that state.

Prefer a small settled-timestep implementation first. Parallel/JIT execution can
initially refuse an unsupported interactive mode rather than silently diverge.
Preserve original diagnostics and do not suppress compile/runtime errors for the
sake of an embedding API.

## Budgets, cancellation, ownership

Bound scheduler work (events/delta iterations), not just simulated time: a
zero-delay loop can consume unlimited wall time. Check cancellation at safe
points, including nested execution paths. Define whether a budget stop is
resumable or leaves a failed session. Thread/process isolation remains the
embedder's responsibility; running Rust does not sandbox HDL host I/O.

Avoid promising independent concurrent sessions until process-global
VPI/configuration is audited. Start with a documented serialized owner-thread
contract. Never invoke user callbacks while mutable engine state is borrowed;
buffered changes simplify this.

## Acceptance tests

Compare a complete one-shot run with multiple advances at the same final
timestamp:

- Counters/registers, reset/enable changes, PWM duty boundaries, wraparound.
- Input injection while paused, including combinational wakeup and posedge
  detection; document clock-edge races.
- Delayed blocking/nonblocking assignments, inactive/NBA ordering, clocks, and
  nested task delays spanning a deadline.
- Four-state startup, X/Z vectors, tri-state release/contention, wide values.
- Independent hierarchical instances and stable handles.
- Exact timescale conversion with sub-nanosecond events.
- Initialization/start callbacks exactly once; no final/end callbacks or trace
  shutdown on pause; explicit finish exactly once.
- Value-change stream and VCD equivalence, including pauses with no events.
- Quiescence, explicit `$stop`/`$finish`, fatal diagnostics, budget exhaustion,
  and cancellation.

Use RGate's HDL fixtures as a starter, but add upstream scheduler-specific
regression tests and compare selected semantics against Icarus. Keep ordinary
xezim CLI tests passing. External/random/file-driven designs are not valid
deterministic-replay equivalence tests without controlled inputs.

## Contribution sequence

1. Ask maintainers whether a retained-session embedding API fits their roadmap;
   share this use case and minimal semantics.
2. Submit a focused lifecycle split with one-shot equivalence tests.
3. Add settled-timestep/deadline execution and stop reasons.
4. Add safe deposit and stable inspection handles.
5. Add buffered change observation and bounded/cancellable work.
6. Adapt RGate behind its backend boundary, test the same fixture set, then
   decide whether to use it alongside or instead of Icarus.

A narrow prototype may take days to weeks; a robust supported API can take weeks
or longer depending on scheduler coupling. This is not an implementation
estimate from upstream. A permanent fork means ongoing merges and correctness
maintenance, so prefer a temporary fork leading to small reviewable upstream
PRs. No upstream publication has been performed by this task.
