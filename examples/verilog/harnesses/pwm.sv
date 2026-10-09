// RGate evaluation harness; GPL-3.0-or-later.
`timescale 1ns/1ps
module pwm_demo;
    reg clk=0; always #50 clk=~clk;
    reg [7:0] duty=0;
    wire led;
    pwm dimmer(.clk(clk),.duty(duty),.pwm_out(led));
endmodule
