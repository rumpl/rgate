// RGate UI binding harness, GPL-3.0-or-later. Upstream PWM remains unmodified.
`timescale 1ns/1ps
module pwm_signals(input [7:0] count,duty,input CLK,RESET_N,input led);
    wire zero=0,carry_one=1;
    wire [7:0] one=1,next_count=count+8'd1,not_duty=~duty;
    wire next_count_carry=(count==255);
    wire [8:0] subtraction={1'b0,count}+{1'b0,not_duty}+9'd1;
    wire [7:0] difference=subtraction[7:0];
    wire greater_or_equal=subtraction[8];
endmodule
module pwm_rgate;
    reg CLK=0; always #50 CLK=~CLK;
    reg RESET_N=0;
    reg [7:0] duty=128;
    wire led;
    wire [7:0] count=dimmer.cnt;
    pwm dimmer(.clk(CLK),.duty(duty),.pwm_out(led));
    // The schematic has an extra teaching reset. Hold the upstream register,
    // then release it to its own RTL; do not implement the counter in the host.
    always @(RESET_N) if(!RESET_N) force dimmer.cnt=0; else release dimmer.cnt;
    initial force dimmer.cnt=0;
    pwm_signals dimmer_view(count,duty,CLK,RESET_N,led);
endmodule
