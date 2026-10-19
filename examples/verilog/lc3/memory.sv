`timescale 1ns/1ps
module LC3MemoryHDL (
    input wire CLK,
    input wire RESET_N,
    input wire [15:0] ADDRESS,
    input wire [15:0] WRITE_DATA,
    input wire WRITE,
    output wire [15:0] READ_DATA,
    output wire [15:0] RESULT,
    output reg [23:0] CONSOLE,
    output reg [7:0] TX,
    output reg [15:0] TX_COUNT
);
    reg [15:0] memory [0:65535];
    integer i;
    // Embedded image keeps the example independent of the simulator's directory.
    initial begin
        for (i = 0; i < 65536; i = i + 1) memory[i] = 0;
        memory[16'h0021] = 16'h3100;
        memory[16'h3000] = 16'h5020;
        memory[16'h3001] = 16'h5260;
        memory[16'h3002] = 16'h1265;
        memory[16'h3003] = 16'h1001;
        memory[16'h3004] = 16'h127F;
        memory[16'h3005] = 16'h03FD;
        memory[16'h3006] = 16'h3019;
        memory[16'h3007] = 16'h2019;
        memory[16'h3008] = 16'hF021;
        memory[16'h3009] = 16'h2018;
        memory[16'h300A] = 16'hF021;
        memory[16'h300B] = 16'h2017;
        memory[16'h300C] = 16'hF021;
        memory[16'h300D] = 16'hF025;
        memory[16'h3020] = 16'h0000;
        memory[16'h3021] = 16'h0048;
        memory[16'h3022] = 16'h0049;
        memory[16'h3023] = 16'h000A;
        memory[16'h3100] = 16'h2402;
        memory[16'h3101] = 16'h7080;
        memory[16'h3102] = 16'hC1C0;
        memory[16'h3103] = 16'hFE06;
    end
    assign READ_DATA = memory[ADDRESS];
    assign RESULT = memory[16'h3020];
    always @(posedge CLK or negedge RESET_N) begin
        if (!RESET_N) begin
            CONSOLE <= 0;
            TX <= 0;
            TX_COUNT <= 0;
        end else if (WRITE) begin
            memory[ADDRESS] <= WRITE_DATA;
            if (ADDRESS == 16'hfe06) begin
                CONSOLE <= {CONSOLE[15:0], WRITE_DATA[7:0]};
                TX <= WRITE_DATA[7:0];
                TX_COUNT <= TX_COUNT + 1;
            end
        end
    end
endmodule
