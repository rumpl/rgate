# Getting started

[Documentation home](../README.md) · [Next: Interface](interface.md)

Open RGate on your desktop, or
[launch it in your browser](https://rumpl.github.io/rgate/app/). The starting
circuit is a full adder, with three switches and two output LEDs.

## Run your first circuit

![Full adder ready for editing](../images/editor.png)

1. Press **Space** or click Play.
2. Click `A`, `B`, and `Cin` to change the inputs.
3. Watch the Sum and Cout LEDs. The Nets panel shows the signal values too.
4. Click `○` beside Sum or Cout to record a waveform.
5. Press Space to pause, then use **F6** to step through the next change.
6. Click Stop to return to editing.

## Open an example

Choose **File → Open…** and select a `.rgate` file. The
[example circuits](../examples/README.md) include adders, counters, a shift
register, a Verilog PWM dimmer, an LC-3 computer, and a VGA controller.

To use a linked example, download its `.rgate` file, then open it in RGate. In
the browser, Open lets you select a file from your device.

## Build a small circuit

1. Choose **File → New circuit**.
2. Search Components for **Switch**. Drag it onto the canvas.
3. Add a second switch, an **AND** gate, and an **LED**.
4. Press **W**. Click a switch output, then an AND input. Connect the second
   switch to the other input and the AND output to the LED.
5. Press **V** or Escape to return to selection.
6. Press Space and toggle the switches. The LED lights when both inputs are
   high.
7. Choose **File → Save** to keep your circuit.

Set component widths in Properties before connecting buses. Use
[selection and wire dragging](interface.md#selection-and-moving) to adjust the
layout, and [Arrange](interface.md#find-and-arrange-circuits) to align
components.

## Write a Verilog module

In the desktop app, choose **Module → New Verilog module…**. Start with the
counter template, edit its source in the **Edit** tab, and run it with RGate.
You can place the module in a schematic and connect switches, a clock, and LEDs.
Follow the [Verilog guide](verilog-modules.md) for a complete walkthrough.

## Keep your work

On desktop, Save writes a `.rgate` document. In the browser, Save downloads one.
RGate remembers your panel sizes, theme, and views, and offers recovery of
unsaved edits when you reopen it. See
[files and recovery](files-and-workspace.md).
