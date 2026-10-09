# Verilog playground

Downloaded, unmodified upstream RTL with retained licenses; exact revisions and
SHA-256 hashes are in [sources.json](sources.json). No Rust CPU/UART/timing
emulators are involved.

| Project                                                       | Downloaded RTL                         | License          | Evaluation                                                            |
| ------------------------------------------------------------- | -------------------------------------- | ---------------- | --------------------------------------------------------------------- |
| [Project F](https://github.com/projf/projf-explore)           | `display_480p.sv`, `lfsr.sv`, `pwm.sv` | MIT              | Small parameterized raster; LFSR reset/enable and expected outputs    |
| [verilog-uart](https://github.com/alexforencich/verilog-uart) | `uart.v`, `uart_rx.v`, `uart_tx.v`     | MIT              | Send `H` through serial loopback, verify byte and errors              |
| [PicoRV32](https://github.com/YosysHQ/picorv32)               | `picorv32.v`                           | ISC              | Execute a five-instruction ROM, store 12, trap on `ebreak`            |
| RGate fixture                                                 | `harnesses/four-state.sv`              | GPL-3.0-or-later | Tri-state release/contention, delayed assignment, 128-bit observation |

RGate-written harnesses are separate in `harnesses/`. JSON projects explicitly
select sources, top module, signal paths/widths, and scripted input/check
actions. These projects **do not import as editable schematics**; they run
through a separate experimental native HDL runner.

Install Icarus (`brew install icarus-verilog` on macOS). From the repository
root:

```sh
cargo run -p rgate-hdl --features icarus -- examples/verilog/projectf.json --vcd /tmp/projectf.vcd
cargo run -p rgate-hdl --features icarus -- examples/verilog/uart.json --vcd /tmp/uart.vcd
cargo run -p rgate-hdl --features icarus -- examples/verilog/picorv32.json --vcd /tmp/picorv32.vcd
cargo run -p rgate-hdl --features icarus -- examples/verilog/four-state.json --vcd /tmp/four-state.vcd
cargo run -p rgate-hdl --features icarus -- examples/verilog/pwm.json --vcd /tmp/pwm.vcd
cargo test -p rgate-hdl --features icarus,xezim
```

The runner prints observed values and fails a mismatched check with a nonzero
exit. Only `probe` signals are exported to VCD. Current schematic UI has no VCD
import; use an external waveform viewer. The default runner uses persistent
Icarus VPI. Add `--features icarus,xezim` and place `--xezim` before the project
path to select the older replay adapter (Rust 1.92+ required).

**Trusted HDL only:** this is not a sandbox. HDL system tasks may access files
or invoke commands. The optional xezim adapter repeats execution from time zero,
including side effects. Avoid testbenches with `$finish`, `$stop`, `final`,
random/external state, or dump/filesystem tasks. Read the
[backend evaluation](../../docs/guides/hdl-backend.md) for the substantial
stepping and performance limitations.

## PWM shared fixture

`pwm.json` runs the unmodified Project F 8-bit PWM LED dimmer. Regression tests
verify 0, 1, 64, 128, 192, and 255 high samples per 256 rising edges. The
editable `../pwm-dimmer.rgate` counterpart uses registers/adders/NOT gates, not
an HDL interpreter; open **File → PWM LED dimmer**. Its LED displays the
instantaneous digital state, not averaged brightness.
