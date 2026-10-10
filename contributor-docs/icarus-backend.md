# Persistent Icarus Verilog backend prototype

[HDL evaluation](hdl-backend.md) · [HDL fixtures](../examples/verilog/README.md)

Unlike the xezim replay prototype, this adapter compiles once and controls **one
persistent `vvp` child process** through a small C VPI plugin. The Rust adapter
implements `SimulationBackend`, with no unsafe Rust or in-process FFI. An opt-in
desktop PWM binding now connects it to editor inputs, LEDs, hierarchy, and
waveforms. The [Verilog module workflow](../docs/guides/verilog-modules.md) also
runs stored source modules and their supported schematic parents. The original
PWM binding remains an explicit example.

## Try PWM

```sh
brew install icarus-verilog
cargo run -p rgate-hdl --features icarus -- examples/verilog/pwm.json --vcd /tmp/pwm.vcd
cargo test -p rgate-hdl --features icarus,xezim
```

Icarus **13.0** was tested on macOS arm64. `iverilog`, `iverilog-vpi`, `vvp`,
and a C compiler must be on PATH. The plugin builds in a temporary directory for
each new backend. No server is needed. The desktop launcher enables Icarus
support by default. The backend remains native-only; browser builds exclude it.
Icarus tools are needed for HDL simulation, not ordinary schematic simulation.
Use `cargo run -p rgate --no-default-features` for a desktop build without the
backend.

## Try it in the desktop editor

```sh
cargo run -p rgate
# Or build the macOS bundle:
./scripts/bundle-macos.sh dev
```

1. Open **File → PWM LED dimmer — Icarus HDL (experimental)**.
2. Start simulation or Clock step. Compilation happens once at startup; the live
   bar reads **Icarus VPI · PWM prototype**.
3. Click **RESET_N** to release reset. Double-click **DUTY** and enter a
   hexadecimal byte, e.g. `40` (25%), `80` (50%), or `C0` (75%). This changes
   the HDL runtime, not the saved schematic.
4. Probe `led`, `count`, and `duty` using the Nets panel or wire double-clicks.
   LEDs show instantaneous digital states; the counter display follows the HDL
   register. Run is intentionally slow for inspection, not an averaged LED
   brightness model.
5. Step visits the next HDL timestep; Clock step advances 100 ns. Double-click
   **dimmer** or use the live hierarchy tree to inspect bound internal signals
   without restarting.
6. Stop destroys the child runtime. Starting again recompiles a fresh instance.
   **File → PWM LED dimmer** selects the ordinary Rust simulator instead.

The schematic is an explicit visual counterpart of the upstream PWM RTL, **not
automatically synthesized or imported HDL**. A small HDL wrapper adds the
schematic's reset using force/release on the upstream counter and exposes
derived debug signals. Core counting and PWM decisions remain in the unmodified
Project F RTL. Only this exact bundled circuit is supported: after edits, the
Icarus prototype refuses to run stale bindings. Reopen the example, or choose
Rust simulation to work on an edited copy. Backend choice is session-only and is
not persisted in `.rgate` files or recovery settings.

Compilation and synchronous runtime requests currently block the UI until
completion or runtime timeout; background startup/cancellation remain future
work. Other circuits, memory/TTY/VGA peripherals, and browser builds remain on
the Rust backend. Finder launches on macOS search Homebrew's common tool paths
if the shell PATH is absent.

## Why this backend

Vitamin **0.2.1**, revision `35993da40e8649128ef2152b824b365535e86c6d`, was
inspected first. Its public `sim_engine::simulate` and `simulate_capture` APIs
create/run a runtime to completion or a limit. `Scheduler` and `SimState` are
private. There is no public retained-runtime step/input/read API at that
revision. This does not mean vitamin cannot gain one; it means another replay
adapter would not solve our requirement.

Icarus offers VPI callbacks and hierarchical handles. We pause at a settled
read-only boundary and wait for a command from Rust. Resuming returns control to
the same scheduler rather than replaying HDL.

| Requirement         | Prototype result                                                                                      |
| ------------------- | ----------------------------------------------------------------------------------------------------- |
| Pause/resume        | Same child runtime and HDL state continue; initialization and final blocks do not rerun at pauses     |
| Step                | Advances to the next scheduled **timestep**, settling that slot; not a single delta/event instruction |
| Advance             | Relative integer-nanosecond delay, then settled snapshot                                              |
| Live input          | VPI deposit into a bound harness variable, with propagation before return                             |
| Hierarchy           | Recursively enumerates module nets/registers and exposes full paths and widths                        |
| Four-state          | X/Z, driver contention, released tri-state bus, mixed 128-bit vectors pass                            |
| Changes             | VPI value-change callbacks feed bounded RGate traces and VCD export                                   |
| Sub-nanosecond time | Exact ticks/scale exposed; next-step test covers 500 ps edges                                         |
| Isolation           | Child runtime is killable, has a 10-second response deadline, and is killed/reaped on drop            |

The downloaded VGA/LFSR, UART loopback, PicoRV32 ROM, four-state fixture, and
PWM all pass. PWM verifies **0, 1, 64, 128, 192, and 255 high samples per 256
rising edges** after live duty changes. Keep these fixtures when testing other
backends.

## Important boundaries

- A read-only callback cannot safely schedule writes in the same time slot.
  `set_input` therefore deposits **one simulator-precision tick later**,
  settles, then returns. At 1 ps precision, two input calls add 2 ps. Use exact
  `time_ticks()` / `ticks_per_nanosecond()` when this matters; trait time and
  VCD currently truncate to integer ns. Avoid writes coincident with clock
  edges.
- `step` can wait indefinitely in an idle design with no future event; the
  response timeout kills it rather than claiming an event occurred. There is no
  artificial heartbeat.
- Probes record changes from registration onward, not historical activity.
  Multiple same-ns transitions can appear because RGate timestamps are ns;
  callbacks can include intermediate delta changes.
- Inputs are deposits, not force/release. Binding a signal as interactive does
  not validate HDL direction or guarantee that other HDL drivers won't overwrite
  it. Harness variables are the supported pattern.
- Hierarchy enumeration currently covers scalar/packed nets and registers under
  modules, not unpacked arrays, memories, all generate scopes, or arbitrary VPI
  object kinds. Explicit bindings require simple dot-separated identifiers and
  widths up to 4096 bits, with at most 256 bindings.
- Runtime failure/timeout makes the instance unusable. Compilation subprocesses
  have no timeout yet. HDL stdout is drained but not presented in the UI; stderr
  is inherited.
- Files are compiled as supplied source strings in an isolated temporary working
  directory. Includes, external memory images, defines, and top parameter
  configuration are not exposed beyond normal HDL wrappers.
- The C bridge uses a dedicated prefixed text protocol on stdout. It is an
  experimental trusted-design protocol, not protection against malicious HDL
  forging messages.
- **Not sandboxed.** A child process is not a security boundary: HDL system
  tasks can write files or invoke host commands. Run trusted HDL only. Child
  cleanup does not guarantee cleanup of arbitrary subprocesses launched by HDL.
- Stored [Verilog modules](../docs/guides/verilog-modules.md) can now run
  independently or in supported schematic parents. This does not implement
  automatic HDL synthesis/import or video peripheral hookup.

## Current assessment

**Icarus is the stronger interactive prototype so far.** It provides the runtime
lifecycle and debug hooks that xezim/vitamin's inspected public APIs lack.
Before making it a production editor backend, add stable protocol framing,
configurable execution budgets, compiler cancellation, direction/type-aware
input binding, fuller hierarchy discovery, precise timestamps, and
GUI/peripheral integration. Test more RTL and cross-check behavior before
choosing permanently.
