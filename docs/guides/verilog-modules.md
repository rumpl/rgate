# Verilog modules

[Modules](modules.md) · [Simulation](simulation.md) · [Waveforms](waveforms.md)

Write Verilog or SystemVerilog in RGate’s desktop source editor, run the module,
and watch its signals. Combine source modules with schematic clocks, switches,
DIP inputs, and LEDs to create an interactive circuit.

## Try the PWM example

1. Download [pwm-verilog.rgate](../../examples/pwm-verilog.rgate) and open it
   with **File → Open…**.
2. Select `pwm` in Modules and choose **Edit** to see its source.
3. Return to `main` and click Play. The clock drives the `dimmer` instance.
4. Double-click **DUTY** and enter a hexadecimal byte: `40` for 25%, `80` for
   50%, or `C0` for 75% duty.
5. Probe `pwm_out` to see the pulse width in Waveforms.
6. Open the live `dimmer` instance and probe `cnt` to follow the counter.

Stop simulation, change the PWM expression, and run again to see your change.
Source edits become part of the document as you type; **File → Save** keeps them
in the `.rgate` file.

## Create a module

Choose **Module → New Verilog module…**. Give it a name that matches the source
module declaration. The dialog provides a counter template; you can also paste
source of your own.

For example, name the module `counter` and use:

```verilog
module counter (
    input wire clk,
    input wire reset,
    input wire enable,
    output reg [7:0] count
);
    always @(posedge clk)
        if (reset)
            count <= 0;
        else if (enable)
            count <= count + 1;
endmodule
```

Declare the signals RGate should expose, one per line in the interface field:

```text
input clk 1
input reset 1
input enable 1
output count 8
```

Each line has a direction, a name, and a width in bits. Use `input`, `output`,
`inout`, or `signal`; `signal` adds an internal signal for inspection. Match the
names and widths to the source. Use **Interface…** to update this list later.

Click Apply, then edit the source in the **Edit** tab. The editor provides
syntax colors, line numbers, scrolling, selection, indentation, search, and
undo/redo. You can keep a work-in-progress source file and run it when ready.

## Connect it to a schematic

1. Switch to a schematic parent, such as `main`.
2. Find your module under **Module instances** in Components and place it.
3. Connect a Clock to `clk`, switches to `reset` and `enable`, and an eight-bit
   LED to `count`.
4. Start simulation. Assert `reset=1` for a clock edge, then set `reset=0` and
   `enable=1`.
5. Watch the counter advance. Pause or clock-step to inspect individual updates.

An instance uses the definition’s declared ports. Editing the definition updates
its instances. Stop simulation before source edits; disconnect affected
instances before changing connected port widths.

## Run a module on its own

Select the source module and start simulation. Declared inputs begin at zero.
Select an input in Nets, then choose **Module → Set selected HDL input…** and
enter its value in hexadecimal.

For the counter above, set `reset=1` and change `clk` from `0` to `1` to clear
`count`. Set the clock back to `0`, release reset, set `enable=1`, then raise
the clock again. `count` becomes 1. A schematic Clock makes repeated cycles
easier.

## Inspect signals and diagnose source

Click `○` beside a declared signal to record it in Waveforms. Outputs and
internal observations appear in Nets with their live values. Navigate to a
specific schematic instance to inspect that copy while the simulation continues.

If a run reports an error, read Messages for the source location or signal name.
Check the module declaration, interface names, and bus widths, correct the
source, and run again. Run source you trust.

## Save and share

A `.rgate` document keeps your source, signal interface, schematic instances,
and layout together. **File → Export Verilog…** exports the circuit and source
modules for use in other Verilog tools.

Use desktop RGate for Verilog editing and execution. The browser app provides
schematic editing and simulation, plus source viewing and editing through its
source dialog.
