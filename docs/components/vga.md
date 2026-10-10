# VGA display

[Component index](README.md) · [Tiny VGA example](../examples/tiny-vga.md)

![VGA sink displaying the circuit-generated test pattern](../images/tiny-vga.png)

Connect a raster circuit to this display to watch its RGB444 pixels and
synchronization signals.

| Pin       | Direction             | Meaning                                                  |
| --------- | --------------------- | -------------------------------------------------------- |
| PCLK      | Input, scalar         | Capture on rising edge                                   |
| HSYNC     | Input, scalar         | Active-low line synchronization                          |
| VSYNC     | Input, scalar         | Active-low frame synchronization                         |
| DE        | Input, scalar         | Capture a pixel only when high                           |
| R / G / B | Input, four bits each | RGB444 color, expanded to eight-bit channels for display |

The display shows a **32×24** image. Pixel position advances within active
lines; falling HSYNC starts the next line after active pixels, and falling VSYNC
resets the frame position. Inconsistent sync/active-area dimensions increment an
error count. Unknown pixel data appears magenta.

During simulation double-click the component to view the captured pixels, frame
count, pixel count, and errors. Run/Pause preserves the same circuit runtime.
**Advance one tiny frame** advances 1536 root clock periods as a convenience for
the Tiny VGA example. Release reset is offered for recognized low root RESET_N.

Save your circuit to keep the display wiring. Start a fresh run to capture a new
image from the connected pixel and timing signals.
