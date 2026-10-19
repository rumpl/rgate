`timescale 1ns/1ps
module LC3ControlHDL (
    input wire CLK,
    input wire RESET_N,
    input wire [3:0] OPCODE,
    input wire HALT_TRAP,
    output reg [7:0] STATE,
    output reg [31:0] CONTROL
);
    // Same control-word layout and microstates as the schematic LC-3 ROM.
    wire [7:0] dispatch = 8'd16 + {OPCODE, 3'b000};
    always @(posedge CLK or negedge RESET_N) begin
        if (!RESET_N) STATE <= 0;
        else if (CONTROL[29] && HALT_TRAP) STATE <= 8'hff;
        else if (CONTROL[28]) STATE <= dispatch;
        else STATE <= CONTROL[7:0];
    end
    always @* begin
        case (STATE)
            8'd0: CONTROL = 32'h00000e01;
            8'd1: CONTROL = 32'h00002102;
            8'd2: CONTROL = 32'h00001303;
            8'd3: CONTROL = 32'h00000a04;
            8'd4: CONTROL = 32'h10000001;
            8'd16: CONTROL = 32'h01000c01;
            8'd24: CONTROL = 32'h00018001;
            8'd32: CONTROL = 32'h00002421;
            8'd33: CONTROL = 32'h00018301;
            8'd40: CONTROL = 32'h00002429;
            8'd41: CONTROL = 32'h4000452a;
            8'd42: CONTROL = 32'h00020001;
            8'd48: CONTROL = 32'h00808131;
            8'd49: CONTROL = 32'h08200c01;
            8'd56: CONTROL = 32'h00058001;
            8'd64: CONTROL = 32'h00502441;
            8'd65: CONTROL = 32'h00018301;
            8'd72: CONTROL = 32'h00502449;
            8'd73: CONTROL = 32'h4000454a;
            8'd74: CONTROL = 32'h00020001;
            8'd88: CONTROL = 32'h00098001;
            8'd96: CONTROL = 32'h00002461;
            8'd97: CONTROL = 32'h00002362;
            8'd98: CONTROL = 32'h00018301;
            8'd104: CONTROL = 32'h00002469;
            8'd105: CONTROL = 32'h0000236a;
            8'd106: CONTROL = 32'h4000456b;
            8'd107: CONTROL = 32'h00020001;
            8'd112: CONTROL = 32'h00000d01;
            8'd128: CONTROL = 32'h00018401;
            8'd136: CONTROL = 32'h00808189;
            8'd137: CONTROL = 32'h2000278a;
            8'd138: CONTROL = 32'h00000b01;
            default: CONTROL = 32'h020000ff;
        endcase
    end
endmodule
