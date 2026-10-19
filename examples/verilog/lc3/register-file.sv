`timescale 1ns/1ps
module LC3RegisterFileHDL (
    input wire CLK,
    input wire RESET_N,
    input wire WRITE,
    input wire [2:0] DR,
    input wire [2:0] SR1,
    input wire [2:0] SR2,
    input wire [15:0] DATA,
    output wire [15:0] A,
    output wire [15:0] B,
    output wire [15:0] R0,
    output wire [15:0] R1,
    output wire [15:0] R2,
    output wire [15:0] R3,
    output wire [15:0] R4,
    output wire [15:0] R5,
    output wire [15:0] R6,
    output wire [15:0] R7
);
    reg [15:0] registers [0:7];
    integer i;
    always @(posedge CLK or negedge RESET_N) begin
        if (!RESET_N) begin
            for (i = 0; i < 8; i = i + 1)
                registers[i] <= 16'b0;
        end else if (WRITE) begin
            registers[DR] <= DATA;
        end
    end
    assign A = registers[SR1];
    assign B = registers[SR2];
    assign R0 = registers[0];
    assign R1 = registers[1];
    assign R2 = registers[2];
    assign R3 = registers[3];
    assign R4 = registers[4];
    assign R5 = registers[5];
    assign R6 = registers[6];
    assign R7 = registers[7];
endmodule
