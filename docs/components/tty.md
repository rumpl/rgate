# TTY terminal

[Component index](README.md) · [LC-3 example](../examples/lc3.md)

![TTY terminal as rendered in RGate](../images/components/tty.png)

TTY provides host text output and queued keyboard/text input through **parallel
byte buses**. It is not a UART: no baud rate or serial start/stop bits. Width is
fixed at eight bits. Two protocols are available in Properties; choose one
before wiring because the pins change.

![LC-3 output in the live terminal](../images/terminal.png)

## Default byte-strobe protocol

| Pin   | Direction      | Meaning                                      |
| ----- | -------------- | -------------------------------------------- |
| TX    | Input, 8 bits  | Byte the circuit wants to print              |
| WR    | Input, scalar  | **Rising edge** captures one TX byte         |
| RX    | Output, 8 bits | First queued input byte, or zero if empty    |
| READY | Output, scalar | 1 when input queue is nonempty               |
| RD    | Input, scalar  | **Rising edge** consumes the current RX byte |

### Output example: print A

1. Drive TX with `0x41` (ASCII A).
2. Keep WR=0 while data settles.
3. Raise WR to 1. One byte is captured.
4. Lower WR before the next byte. Holding it high does **not** repeatedly print.

Unknown TX data at the write edge produces `?`. Writing byte 0x0A produces a
newline. Multi-byte UTF-8 sequences can display Unicode when complete; ASCII is
easiest for simple circuits.

### Input

During simulation double-click TTY. Enter text in **Send UTF-8 bytes**, then
Apply. No newline is appended automatically. The circuit reads the current RX
value while READY=1, pulses RD, and reads the next byte. Non-ASCII characters
are multiple UTF-8 bytes. READY=0 means RX is not a valid queued byte even
though its value is zero.

## TkGate-style handshake protocol

Enable **TkGate TD/RD/RTS/CTS/DSR/DTR protocol** in TTY Properties.

| Pin | Direction         | Meaning in RGate                                                    |
| --- | ----------------- | ------------------------------------------------------------------- |
| RD  | Input, **8 bits** | Circuit-to-terminal byte (different from scalar RD in default mode) |
| DSR | Input, scalar     | Rising edge schedules capture of RD                                 |
| DTR | Output, scalar    | Acknowledges known DSR state                                        |
| TD  | Output, 8 bits    | First queued terminal-to-circuit byte                               |
| CTS | Input, scalar     | Low requests input; rising edge schedules consumption               |
| RTS | Output, scalar    | 1 while CTS is low and queued input is available                    |

Capture/consumption and output acknowledgement use scheduled **10 ns delays**.
This is a built-in digital handshake, not the Tcl host-plugin engine or a serial
terminal. Prepare RD before raising DSR; allow acknowledgement/data to settle.
For receive, enqueue text, lower CTS, read TD when RTS=1, then raise CTS to
consume it.

## Live terminal controls

The dialog keeps simulation live. **Run/Pause** and **Clock step** operate on
the existing simulation, including a TTY inside a live child instance. Time
displayed is simulated ns.

An empty terminal says no bytes have arrived; it isn't proof the TTY is broken.
For a recognized root `RESET_N` switch held low, the dialog warns the circuit is
in reset and offers **Release reset**. In the LC-3 example that must be done
before instruction execution.

Apply enqueues text and closes the dialog. Cancel closes without sending.
Closing the dialog does not itself save circuit changes or persist terminal
state. Stop/restart simulation resets queues and output. Input queues and
retained output are bounded; huge input is rejected rather than growing
indefinitely.

TTY host I/O is supported in native `.rgate` saves, but **executable Verilog
export refuses TTY**, since there is no HDL peripheral backend. The LC-3 CPU
modules without their root terminal can be exported separately. Browser and
desktop share the TTY simulator; no Tcl plugins are required.
