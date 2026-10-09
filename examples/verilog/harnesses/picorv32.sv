// RGate evaluation harness; GPL-3.0-or-later.
`timescale 1ns/1ps
module picorv32_demo;
    reg clk=0; always #5 clk=~clk;
    reg reset_n=0;
    wire trap,mem_valid,mem_instr;
    wire [31:0] mem_addr,mem_wdata;
    wire [3:0] mem_wstrb;
    reg [31:0] mem_rdata;
    reg [31:0] result=0;
    // The CPU is upstream Verilog. This is only a tiny combinational program ROM.
    always @* case(mem_addr)
        0: mem_rdata=32'h00500093;  // addi x1,x0,5
        4: mem_rdata=32'h00700113;  // addi x2,x0,7
        8: mem_rdata=32'h002081b3;  // add x3,x1,x2
        12:mem_rdata=32'h10302023;  // sw x3,256(x0)
        16:mem_rdata=32'h00100073;  // ebreak
        default:mem_rdata=0;
    endcase
    always @(posedge clk) begin
        if(!reset_n) result<=0;
        else if(mem_valid && mem_wstrb==4'hf && mem_addr==256) result<=mem_wdata;
    end
    picorv32 #(.PROGADDR_RESET(0),.ENABLE_COUNTERS(0)) cpu(
        .clk(clk),.resetn(reset_n),.trap(trap),.mem_valid(mem_valid),
        .mem_instr(mem_instr),.mem_ready(1'b1),.mem_addr(mem_addr),
        .mem_wdata(mem_wdata),.mem_wstrb(mem_wstrb),.mem_rdata(mem_rdata),
        .irq(32'b0));
endmodule
