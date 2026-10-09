# Component reference

[Documentation home](../README.md) ·
[Using the interface](../guides/interface.md)

All **47 built-in palette entries** are documented below. Module instances are
additional user-defined components; `Unsupported` is an import diagnostic, not a
usable built-in.

## Catalog

| Category                                          | Components                                                                                             |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| [Logic](logic.md)                                 | AND, NAND, OR, NOR, XOR, XNOR, Buffer, NOT, Tri-state buffer; Reduction AND, NAND, OR, NOR, XOR, XNOR  |
| [Inputs and displays](inputs-and-displays.md)     | Switch, DIP switch, Clock, Ground, Vdd, LED (five display modes), GPIO peripheral                      |
| [Sequential and memory](sequential-and-memory.md) | Register, D flip-flop, JK flip-flop, RAM, ROM                                                          |
| [Routing and buses](routing.md)                   | Multiplexer, Decoder, Demultiplexer, Splitter, Concatenation, Tap                                      |
| [Arithmetic](arithmetic.md)                       | Adder, Multiplier, Divider, Shift left, Shift right, Arithmetic shift right, Rotate left, Rotate right |
| [Transistors](transistors.md)                     | NMOS, PMOS                                                                                             |
| [VGA display](vga.md)                             | VGA display, digital RGB444 visual sink                                                                |
| [TTY](tty.md)                                     | Terminal with byte-strobe or TkGate-style handshake mode                                               |
| [Annotations](annotations.md)                     | Comment, Frame                                                                                         |
| [Modules](../guides/modules.md)                   | User-defined instances, ports, and symbols                                                             |

Symbol images are cropped screenshots of actual Classic-theme rendering. They
show unwired defaults (plus explicit LED-mode examples), not guaranteed
simulation values. Port labels and size can change with properties.

## Shared behavior

- Pin directions are relative to the component; `I`, `A`, `B`, etc. are inputs,
  while `Z`, `Q`, `S`, etc. are outputs. `D` on RAM is bidirectional. The same
  letter can mean something different on another gate.
- Widths are 1–4096 bits unless a pin is explicitly scalar, address-limited, or
  byte-wide. Display/property fields must fit the configured width; values
  aren't silently accepted beyond it.
- Set widths/layout before wiring. A control's name does not alone establish its
  polarity: consult its component page. Registers/memories use several
  active-low controls; muxes use binary select values.
- Gates have configurable output delay in ns. Most primitive output changes use
  inertial delay: a short pulse can be canceled before it appears. Sources
  update immediately; clocks have their own schedule. TTY handshake mode adds
  scheduled delays.
- Four-state logic applies: `0`, `1`, unknown `X`, floating `Z`. An unconnected
  input generally reads `Z`. Documented defaults on sequential/enable pins are
  exceptions. Conflicting drivers resolve to `X`.
- Initial property values are meaningful for inputs such as Switch/DIP/GPIO.
  Sequential registers start unknown unless reset. Setting a register's generic
  Initial value field is not a register initialization feature.
- RAM/ROM initial contents are saved configuration. Runtime memory edits are
  volatile. TTY output/input queues are runtime state, not saved circuit
  contents.
- Explicit circuit wiring determines behavior. RGate does not execute Verilog
  behavior attached to an imported symbol.

To place a component, use **Make**, click its right-sidebar row then click the
canvas, or drag it from the sidebar. Search can find aliases such as `lshift` as
well as descriptive labels.
