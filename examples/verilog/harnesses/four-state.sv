// RGate evaluation fixture; GPL-3.0-or-later.
`timescale 1ns/1ps
module four_state_demo;
    reg enable=0,conflicting=0;
    wire [3:0] bus;
    assign bus=enable ? 4'b1010 : 4'bzzzz;
    assign bus=conflicting ? 4'b0101 : 4'bzzzz;
    wire [3:0] delayed;
    assign #2 delayed=bus;
    wire [127:0] wide={32{bus}};
endmodule
