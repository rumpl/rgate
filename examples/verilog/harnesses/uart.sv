// RGate evaluation harness; GPL-3.0-or-later.
`timescale 1ns/1ps
module uart_demo;
    reg clk=0; always #5 clk=~clk;
    reg reset=1;
    reg [7:0] tx_data=0;
    reg tx_valid=0;
    reg rx_ready=0;
    wire tx_ready,rx_valid;
    wire [7:0] rx_data;
    wire serial,tx_busy,rx_busy,overrun,frame_error;
    uart serial_port(.clk(clk),.rst(reset),.s_axis_tdata(tx_data),
        .s_axis_tvalid(tx_valid),.s_axis_tready(tx_ready),
        .m_axis_tdata(rx_data),.m_axis_tvalid(rx_valid),.m_axis_tready(rx_ready),
        .rxd(serial),.txd(serial),.tx_busy(tx_busy),.rx_busy(rx_busy),
        .rx_overrun_error(overrun),.rx_frame_error(frame_error),.prescale(16'd2));
endmodule
