// RGate LED chaser example. GPL-3.0-or-later.
`timescale 1ns/1ps
module led_chaser (
    input wire clk,
    input wire reset,
    input wire enable,
    input wire reverse,
    input wire [3:0] speed,
    output reg [7:0] leds
);
    reg [3:0] divider;

    always @(posedge clk) begin
        if (reset) begin
            leds <= 8'b00000001;
            divider <= 0;
        end else if (enable) begin
            if (divider >= speed) begin
                divider <= 0;
                if (reverse)
                    leds <= {leds[0], leds[7:1]};
                else
                    leds <= {leds[6:0], leds[7]};
            end else begin
                divider <= divider + 1;
            end
        end
    end
endmodule
