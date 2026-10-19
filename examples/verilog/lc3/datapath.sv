`timescale 1ns/1ps
module LC3DatapathHDL (
    input wire CLK,
    input wire RESET_N,
    input wire [31:0] CONTROL,
    input wire [15:0] MEM_READ,
    input wire [15:0] A,
    input wire [15:0] B,
    output wire [2:0] DR,
    output wire [2:0] SR1,
    output wire [2:0] SR2,
    output wire WRITE_REG,
    output reg [15:0] BUS,
    output reg [15:0] PC,
    output reg [15:0] IR,
    output reg [15:0] MEM_ADDR,
    output reg [15:0] MEM_WRITE,
    output wire WRITE_MEM,
    output reg [2:0] CC,
    output wire [3:0] OPCODE,
    output wire HALT_TRAP,
    output wire HALTED
);
    wire [15:0] signed5 = {{11{IR[4]}}, IR[4:0]};
    wire [15:0] signed6 = {{10{IR[5]}}, IR[5:0]};
    wire [15:0] signed9 = {{7{IR[8]}}, IR[8:0]};
    wire [15:0] signed11 = {{5{IR[10]}}, IR[10:0]};
    wire [15:0] operand_b = IR[5] ? signed5 : B;
    reg [15:0] alu_result;
    reg [15:0] offset;
    wire [15:0] address_base = CONTROL[22] ? A : PC;
    wire [15:0] effective_address = address_base + offset;
    wire [15:0] address_or_jump = CONTROL[27] && !IR[11] ? A : effective_address;
    wire branch_taken = |(CC & IR[11:9]);

    assign DR = CONTROL[23] ? 3'd7 : IR[11:9];
    assign SR1 = CONTROL[30] ? IR[11:9] : IR[8:6];
    assign SR2 = IR[2:0];
    assign WRITE_REG = CONTROL[15];
    assign WRITE_MEM = CONTROL[17] && RESET_N;
    assign OPCODE = IR[15:12];
    assign HALT_TRAP = IR[7:0] == 8'h25;
    assign HALTED = CONTROL[25];

    always @* begin
        case (CONTROL[19:18])
            2'd0: alu_result = A + operand_b;
            2'd1: alu_result = A & operand_b;
            2'd2: alu_result = ~A;
            default: alu_result = A;
        endcase
        case (CONTROL[21:20])
            2'd0: offset = signed9;
            2'd1: offset = signed6;
            2'd2: offset = signed11;
            default: offset = B;
        endcase
        case (CONTROL[10:8])
            3'd0: BUS = alu_result;
            3'd1: BUS = PC;
            3'd2: BUS = PC + 16'd1;
            3'd3: BUS = MEM_READ;
            3'd4: BUS = address_or_jump;
            3'd5: BUS = A;
            3'd6: BUS = 16'h3000;
            default: BUS = {8'b0, IR[7:0]};
        endcase
    end

    always @(posedge CLK or negedge RESET_N) begin
        if (!RESET_N) begin
            PC <= 0;
            IR <= 0;
            MEM_ADDR <= 0;
            MEM_WRITE <= 0;
            CC <= 0;
        end else begin
            if (CONTROL[11] && (!CONTROL[24] || branch_taken)) PC <= BUS;
            if (CONTROL[12]) IR <= BUS;
            if (CONTROL[13]) MEM_ADDR <= BUS;
            if (CONTROL[14]) MEM_WRITE <= BUS;
            if (CONTROL[16]) CC <= {BUS[15], BUS == 0, !BUS[15] && BUS != 0};
        end
    end
endmodule
