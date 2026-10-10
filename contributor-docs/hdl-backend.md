# Experimental HDL backends

[Persistent Icarus VPI evaluation](icarus-backend.md) ·
[Documentation](../docs/README.md) ·
[Downloaded HDL playground](../examples/verilog/README.md)

`rgate-hdl` is a separate crate with an optional **native-only** `xezim`
feature. Normal editor and web builds do not link xezim. It depends on
`rgate-sim` for the `SimulationBackend`, signal traces, errors, and VCD
exporter; dependency direction is not reversed. The GUI normally runs the
schematic simulator; opt-in native
[Verilog modules](../docs/guides/verilog-modules.md) and the
[desktop PWM example](icarus-backend.md#try-it-in-the-desktop-editor) use
Icarus.

## Run the evaluation

Rust **1.92+** is required when enabling xezim (the rest of the workspace
declares 1.88). The pinned version is xezim **0.11.1**, revision
`8082196408aef55792434c7bedf251f8d0dab51a`; upstream pins xezim-core to
`2366c0d555d613a68e53295451938106d95b689f`. No upstream source patches are
needed. Cargo fetches both engines; their native dependencies include libffi.
macOS arm64 has been tested.

```sh
cargo run -p rgate-hdl --features icarus,xezim -- --xezim examples/verilog/uart.json --vcd /tmp/uart.vcd
cargo test -p rgate-hdl --features xezim
```

Change the project to `projectf.json`, `picorv32.json`, or `four-state.json` to
try the other examples. All HDL files, licenses, source revisions, and hashes
are stored under `examples/verilog/`; no servers are required.

## What the prototype establishes

| Capability                      | Result at the pinned revision                                                                                       |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Verilog/SystemVerilog execution | Project F timing/LFSR, UART loopback, and PicoRV32 ROM program pass                                                 |
| Bounded advance                 | Adapter runs to a requested integer-nanosecond observation time                                                     |
| Input changes                   | Journaled hierarchical assignments to harness variables; reset/enable/data changes propagate                        |
| Hierarchical inspection         | Explicit nested bindings such as `timing.x`, `rng.sreg`, `serial_port.uart_tx_inst.busy_reg`, and `cpu.reg_pc` work |
| Four-state logic                | Unknown startup, mixed X/Z vectors, released tri-state buses, driver contention, and 128-bit replication pass       |
| Delays/NBA                      | Delayed continuous assignment and nonblocking counter updates pass; 100 ps precision source also tested             |
| Traces                          | Postponed `$strobe` samples, change deduplication, and RGate VCD export work                                        |
| Native integration              | Adapter implements `SimulationBackend`; default desktop/web build remains unchanged                                 |
| True scheduler step/resume      | **Not implemented**; public xezim API lacks the needed boundary                                                     |
| Editor integration / WASM       | **Not implemented**; xezim feature is native-only                                                                   |

Project F's scaled raster test checks a **480 ns line period**, **40 ns HSYNC
pulse**, and **960 ns VSYNC pulse** at a 10 ns pixel clock. Its upstream default
mode is 640×480, but our harness overrides parameters for a small/fast test.
UART sends `0x48` over its actual serial output connected to its receive input.
PicoRV32 executes `addi`, `add`, `sw`, and `ebreak` entirely in upstream RTL;
the only custom HDL is a tiny program ROM and result observer.

## Replay is not pause/resume

The upstream library exposes full compile/run, signal reads and writes, but not
a public resumable event/timestep API. Calling its public `simulate` method
repeatedly is not a safe substitute: that method performs register-init policy,
end-of-simulation callbacks, final blocks, and trace shutdown.

Our adapter instead regenerates a wrapper and **recompiles/replays from time
zero** on advance, input change, or adding a probe. Input changes are HDL
assignments with timestamped delays; we do not write directly into private
scheduler state. This proves basic integration but is unsuitable as the
production interactive engine.

- `step()` means **advance 1 ns**, not “next queued event.” Sub-nanosecond
  activity is executed by xezim, but RGate traces round timestamps to integer ns
  and may have multiple transitions at the same displayed timestamp.
- Inputs are **deposits into explicit harness variables**, not force/release.
  Place changes away from clock edges and after startup; assignments at time
  zero or exactly at a clock edge can race with HDL initial/active-region
  processes. Marking a binding interactive does not automatically verify its HDL
  direction/type.
- Hierarchy is **explicit path binding**, not automatic module/net enumeration.
  Simple dot-separated identifiers are supported; escaped identifiers, arrays,
  and generated-scope indexing are not yet exposed.
- `$strobe` observes settled values, not every delta-cycle glitch. It does not
  replace the user's singleton `$monitor`. Trace output is captured and also
  printed by upstream xezim.
- Replay restarts all design state and rebuilds trace history. Deterministic
  runs with the same input journal give the same observed state; external
  resources, random seeds, and runtime side effects are not captured/restored.
- xezim still executes `final` blocks at each run limit. Do not use them for
  this prototype. Explicit `$finish`/`$stop`, `$fatal`, and `$error` prevent
  normal interactive continuation when reported by the engine.
- Resource guardrails cap target time at **1,000,000 ns**, **4096 journaled
  input changes**, **256 bound signals**, and **4096 retained transitions per
  probe**. These are not a sandbox, CPU timeout, or a guarantee against
  pathological HDL.
- Calls are serialized because xezim has process-global configuration/VPI state.
  There is no concurrent independent-engine guarantee or worker cancellation
  yet.
- The trait's returned work count is adapter-specific (changed observed
  bindings), not xezim's event count.

## Project format

JSON selects `top`, ordered source file paths relative to the JSON directory,
explicit `signals` (`path`, `width`, optional `input`), and ordered `actions`.
Actions are `advance` (`ns`), `step`, `set`/`check` (`signal`, exact-width
binary `bits`, including x/z), and `probe` (`signal`). Signal binding IDs are
assigned in list order. Look at the bundled JSON rather than assuming arbitrary
Verilog import creates the required bindings.

Included sources must currently be supplied explicitly; there is no exposed
include-directory, define, parameter-override, or external memory-image
configuration. Parameters in the provided wrappers are normal HDL. Runtime
failures leave the adapter's committed time/values/traces unchanged, but
external HDL side effects cannot be rolled back.

## Assessment and next steps

**Promising for native HDL execution, not yet selected as our interactive
backend.** The downloaded RTL and four-state tests work, but replay is expensive
and semantically weaker than continuing a single runtime. Before choosing xezim,
we need a supported upstream pause/resume API with settled-timestep/next-event
boundaries, signal enumeration and safe input injection, change callbacks,
execution budgets/cancellation, and explicit lifecycle ownership. Then test the
same contract against vitamin and decide with evidence.

Do not infer full IEEE conformance, UVM support, physical VGA interoperability,
or general CPU correctness from these small tests. No vitamin adapter has been
added. The [Icarus VPI prototype](icarus-backend.md) now provides persistent
runtime stepping and live inputs instead of replay.

## Safety

Run **trusted HDL only**. xezim is executed in-process and supports host-facing
system tasks; it can write files or invoke commands. The runner does not isolate
it. Replay can repeat every such side effect on each operation. A production
backend needs process isolation and cancellation before accepting arbitrary
downloaded projects.
