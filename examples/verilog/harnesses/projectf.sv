// RGate evaluation harness; GPL-3.0-or-later.
`timescale 1ns/1ps
module projectf_demo;
    reg clk=0; always #5 clk=~clk;
    reg reset=1;
    reg enable=0;
    reg [7:0] seed=8'h01;
    wire [7:0] random_bits;
    lfsr rng(.clk(clk),.rst(reset),.en(enable),.seed(seed),.sreg(random_bits));
    wire hsync,vsync,de,frame,line;
    wire signed [15:0] sx,sy;
    // Use the upstream timing generator with tiny parameters for fast inspection.
    display_480p #(.H_RES(32),.V_RES(24),.H_FP(4),.H_SYNC(4),.H_BP(8),
                  .V_FP(2),.V_SYNC(2),.V_BP(4)) timing(
        .clk_pix(clk),.rst_pix(reset),.hsync(hsync),.vsync(vsync),
        .de(de),.frame(frame),.line(line),.sx(sx),.sy(sy));
endmodule
