#!/usr/bin/env python3
"""Generate a tiny VGA-style raster circuit. No timing/controller code runs here."""
import json
from circuit_builder import Module,signal,ROOT

# Small pedagogical mode, not a monitor-compatible VGA frequency:
# H: 32 active + 4 front + 4 sync + 8 back = 48 pixel clocks
# V: 24 active + 2 front + 2 sync + 4 back = 32 lines
controller=Module("VgaTiming")
for name in ["CLK","RESET_N"]:controller.port(name,1,"input")
for name,width in [("X",6),("Y",5),("HSYNC",1),("VSYNC",1),("DE",1)]:controller.port(name,width,"output")
for name,value,width in [("zero",0,1),("always_load",0,1),("one6",1,6),("one5",1,5),("zero6",0,6)]:controller.const(name,value,width)
controller.register("X","next_x","always_load",6)
controller.equal("end_line","X",47,6)
controller.add("x_plus_one","X","one6",6)
controller.mux("next_x",["x_plus_one","zero6"],"end_line",6)
controller.register("Y","y_plus_one",controller.invert("y_enable_n","end_line"),5)
controller.add("y_plus_one","Y","one5",5) # width-5 overflow gives exactly 32 lines
controller.tap("h_blank","X",6,5,1)
controller.invert("h_active","h_blank")
controller.tap("y_top","Y",5,4,1);controller.tap("y_next","Y",5,3,1)
controller.logic("v_blank","and","y_top","y_next");controller.invert("v_active","v_blank")
controller.logic("DE","and","h_active","v_active","RESET_N")
# H sync at 36..39: high bits X[5:2] == 1001.
controller.tap("h_sync_bits","X",6,2,4)
controller.equal("h_sync_asserted","h_sync_bits",9,4)
controller.invert("HSYNC","h_sync_asserted")
# V sync at 26..27: Y[4:1] == 1101.
controller.tap("v_sync_bits","Y",5,1,4)
controller.equal("v_sync_asserted","v_sync_bits",13,4)
controller.invert("VSYNC","v_sync_asserted")

pattern=Module("PatternMemory")
pattern.port("X",6,"input");pattern.port("Y",5,"input")
for name in ["R","G","B"]:pattern.port(name,4,"output")
pattern.const("zero",0,1)
pattern.tap("pixel_x","X",6,0,5)
pattern.concat("pixel_address",["pixel_x","Y"],[5,5])
pixels=[]
colors=[0xf00,0xf80,0xff0,0x0f0,0x0ff,0x00f,0x80f,0xfff]
for y in range(32):
    for x in range(32):
        color=colors[x//4] if y<12 else ((x//4+y//4)%2)*0xfff
        if y>=24:color=0
        pixels.append(signal(color,12))
pattern.gate("test_pattern_ROM","rom",12,[("A","input",10,"pixel_address"),("D","output",12,"pixel_color"),("OE","input",1,"zero")],address_bits=10,memory=pixels)
for name,offset in [("R",8),("G",4),("B",0)]:pattern.tap(name,"pixel_color",12,offset,4)

main=Module("main")
main.gate("pixel_clock","clock",1,[("Z","output",1,"CLK")],period=100)
main.gate("RESET_N","switch",1,[("Z","output",1,"RESET_N")],initial=0)
main.instance("timing",controller,{name:name for name in ["CLK","RESET_N","X","Y","HSYNC","VSYNC","DE"]})
main.instance("pattern",pattern,{name:name for name in ["X","Y","R","G","B"]})
main.gate("screen","vga",1,[("PCLK","input",1,"CLK"),("HSYNC","input",1,"HSYNC"),("VSYNC","input",1,"VSYNC"),("DE","input",1,"DE"),("R","input",4,"R"),("G","input",4,"G"),("B","input",4,"B")],vga_width=32,vga_height=24)
for name,width in [("X",6),("Y",5),("HSYNC",1),("VSYNC",1),("DE",1)]:
    gate=main.gate("watch_"+name,"led",width,[("I","input",width,name)],led_display="hex" if width>1 else "bit")
    gate["position"]={"x":120.0+len(main.data["gates"])*90,"y":650.0}
for gate,position in zip(main.data["gates"][:5],[(80,100),(80,250),(310,200),(560,200),(850,200)]):gate["position"]={"x":float(position[0]),"y":float(position[1])}
note=main.gate("instructions","comment",1,[]);note["position"]={"x":40.0,"y":780.0}
note["text"]=("Tiny VGA controller — actual counters, gates and ROM.\nStart in reset, then RESET_N → 1. Double-click screen during simulation.\n32×24 active pixels; total 48×32 ticks; active-low HSYNC/VSYNC.\nFrame period = 153600 ns at 100 ns pixel clock.\nTop half: eight color bars; bottom: checkerboard stored in ROM.\nThis scaled teaching mode is not a real 640×480 monitor-compatible signal.")
circuit={"title":"Tiny VGA controller","root":"main","modules":[main.finish(),controller.finish(),pattern.finish()]}
(ROOT/"examples/tiny-vga.rgate").write_text(json.dumps({"format":"rgate","version":1,"circuit":circuit},separators=(",",":"))+"\n")
print("Generated examples/tiny-vga.rgate")
