# Troubleshooting

[Documentation home](../README.md) · [Simulation](../guides/simulation.md)

## Get terminal output

Check the TTY’s data bus and strobe pins against its selected protocol. Prepare
the byte, then apply a rising strobe edge to capture it. In the LC-3 example,
release RESET_N to 1 after reset settles.

Double-click the terminal to see output and use its Run/Pause or Clock step
controls. Probe PC, IR, and HALTED to follow the program.

## Follow unknown or floating signals

**X** points you toward initialization, unknown controls, invalid addresses,
division by zero, or competing drivers. Reset storage and inspect the data and
control inputs at the same live instance.

**Z** indicates a floating or released signal. Check the wiring and the driver’s
enable, CS, OE, and WE controls using the component reference.

## Match connection widths

Set widths before wiring. Check each pin’s purpose: an eight-bit mux can use a
one-bit selector, while an eight-bit shift uses a three-bit amount input.

Use Concat to assemble a bus, Splitter to divide it, and Tap to read a slice.
Disconnect affected pins before changing their widths, then reconnect them.

## Place and inspect modules

Select the parent schematic before placing a child definition. Declare its ports
in Interface so the block has named pins. During simulation, use the exact Tree
path to choose among repeated instances.

## Restore keyboard focus

Click the canvas to use circuit shortcuts. Click the source editor to type
Verilog or use text undo/redo. Escape ends placement, wiring, or a regular
dialog. The component search field captures text while focused.

## Adjust wires and layout

Select the branch you want to move or delete. Drag a horizontal segment up/down
or a vertical segment left/right. Shift-click or box-select for a group.

Use Arrange to align components, distribute them, tidy a module, or route
selected wires. Work in smaller sections for a dense circuit, and use Undo to
compare arrangements. Fit gives an overview; zoom in for detailed work.

## Show waveform history

Add probes before running the part of the circuit you want to inspect. Use Fit
to see recorded history and Follow to track current time. Clear the filter or
expand grouped signals if rows are hidden.

Place the cursor within the retained interval to read its value. A fresh run
starts a fresh recording with your saved probes.

## Correct a Verilog error

Read Messages for the source location or signal name. Check the module name,
syntax, interface declarations, and bus widths. Stop, correct the source, and
run again.

Use desktop RGate to execute Verilog modules. In a source-only module, use **Set
selected HDL input…** to change its declared inputs; use a schematic parent with
a Clock for repeated cycles.

## Recover work and preferences

Use Save regularly. If recovery is offered, choose Recover and save the restored
circuit. Browser users should retain the downloaded file before clearing site
data or switching to another device.

See [files and recovery](../guides/files-and-workspace.md) for preference
locations and recovery controls.

## Open the browser app

Use a current Chrome, Edge, or Firefox browser and open
[RGate online](https://rumpl.github.io/rgate/app/). Read any startup message on
screen and refresh after checking your browser’s graphics settings.

## Report a problem

Include the `.rgate` document, selected module or instance path, steps to
reproduce, platform/browser, a screenshot, and copied Messages text. Save a copy
before simplifying the circuit for your report.
