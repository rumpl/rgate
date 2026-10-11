# Verilog LED chaser

[Examples](README.md) · [Verilog modules](../guides/verilog-modules.md)

Download [verilog-led-chaser.rgate](../../examples/verilog-led-chaser.rgate) and
open it with **File → Open…** in desktop RGate. A Verilog module moves a single
lit bit across an eight-bit LED bar. Switches control reset, pause, and
direction; a DIP input sets the speed.

## Run it

1. Clock-step once with **RESET=1** to initialize the lit bit.
2. Click **RESET** to set it to **0**.
3. Run or clock-step. With **SPEED=3**, the bit moves every four clock cycles.
4. Set **ENABLE=0** to hold the current position; set it back to 1 to continue.
5. Toggle **REVERSE** to change the direction.
6. Double-click **SPEED** and enter a hex digit from `0` to `F`. The bit moves
   every `SPEED + 1` clocks: `0` is fastest, `F` advances every sixteen clocks.

The hex display shows the same output: `01`, `02`, `04`, `08`, `10`, `20`, `40`,
`80`, then back to `01`. Reverse traverses that sequence in the other direction.

## Edit and inspect

Stop simulation, select **led_chaser** in Modules, and open **Edit**. Change the
reset pattern or rotation expression, then return to **main** and run again. The
[standalone source](../../examples/verilog/led-chaser.sv) is also available.

Probe `leds` to record the output. Choose **Simulate → Browse HDL signals…**,
filter for `chaser`, and probe `divider` to watch the speed counter inside the
running module.
