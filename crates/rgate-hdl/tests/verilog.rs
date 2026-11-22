use rgate_core::{GateId, Logic, NetId, Signal};
use rgate_hdl::{HdlProject, HdlSignal, VerilogBackend};
use rgate_sim::SimulationBackend;
fn binding(id: u64, path: &str, width: u16, input: bool) -> HdlSignal {
    HdlSignal {
        net: NetId(id),
        path: path.into(),
        width,
        input: input.then_some(GateId(id)),
    }
}
#[test]
fn cli_runs_project_and_exports_vcd() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/verilog");
    let output_directory = tempfile::tempdir().unwrap();
    let vcd = output_directory.path().join("four-state.vcd");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rgate-hdl"))
        .arg(directory.join("four-state.json"))
        .arg("--vcd")
        .arg(&vcd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("checks passed"));
    assert!(
        std::fs::read_to_string(vcd)
            .unwrap()
            .contains("$timescale 1ns $end")
    );
}

#[test]
fn downloaded_designs_run_without_replay() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/verilog");
    for name in ["projectf", "uart", "picorv32", "four-state", "pwm"] {
        let project = HdlProject::load(&directory.join(format!("{name}.json"))).unwrap();
        let mut backend = project.instantiate(&directory).unwrap();
        project.execute(&mut backend).unwrap();
        assert!(
            backend
                .discovered_signals()
                .iter()
                .any(|s| s.path.starts_with(&format!("{}.", project.top)))
        );
    }
}
#[test]
fn pwm_live_duty_and_next_timestep_match_expected_high_counts() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/verilog");
    let project = HdlProject::load(&directory.join("pwm.json")).unwrap();
    let mut backend = project.instantiate(&directory).unwrap();
    backend.probe(NetId(3)).unwrap();
    backend.advance(30).unwrap();
    assert!(
        backend
            .discovered_signals()
            .iter()
            .any(|s| s.path == "pwm_demo.dimmer.cnt" && s.width == 8)
    );
    for duty in [0, 1, 64, 128, 192, 255] {
        backend
            .set_input(GateId(1), Signal::from_u64(duty, 8))
            .unwrap();
        let mut highs = 0;
        for _ in 0..256 {
            backend.step().unwrap(); // next falling or rising clock boundary
            if backend.time_ticks() % (100 * backend.ticks_per_nanosecond())
                == 50 * backend.ticks_per_nanosecond()
            {
                highs += usize::from(backend.value(NetId(3)).unwrap().to_u64() == Some(1));
            } else {
                backend.step().unwrap();
                highs += usize::from(backend.value(NetId(3)).unwrap().to_u64() == Some(1));
            }
        }
        assert_eq!(highs, duty as usize, "duty {duty}");
        assert_eq!(backend.value(NetId(2)).unwrap().to_u64(), Some(0));
    }
}
#[test]
fn persistent_runtime_does_not_rerun_initial_or_final_and_handles_four_states() {
    let source = r#"`timescale 1ns/1ps
 module child;reg [127:0] data={32{4'b10xz}};endmodule
 module top;
 reg clk=0;always #5 clk=~clk;
 reg [7:0] count=0;always @(posedge clk) count<=count+1;
 reg enable=0,conflict=0;
 wire [3:0] bus;
 assign bus=enable ? 4'b1010 : 4'bzzzz;
 assign bus=conflict ? 4'b0101 : 4'bzzzz;
 reg final_ran=0;final final_ran=1;
 child u();
 endmodule"#;
    let mut backend = VerilogBackend::new(
        vec![source.into()],
        "top",
        vec![
            binding(1, "count", 8, false),
            binding(2, "enable", 1, true),
            binding(3, "conflict", 1, true),
            binding(4, "bus", 4, false),
            binding(5, "final_ran", 1, false),
            binding(6, "u.data", 128, true),
        ],
    )
    .unwrap();
    backend.probe(NetId(4)).unwrap();
    assert_eq!(
        backend.value(NetId(4)).unwrap(),
        &Signal::filled(4, Logic::HighZ)
    );
    backend.step().unwrap();
    assert_eq!(backend.time(), 5);
    assert_eq!(backend.value(NetId(1)).unwrap().to_u64(), Some(1));
    backend.advance(11).unwrap();
    assert_eq!(backend.value(NetId(1)).unwrap().to_u64(), Some(2));
    backend
        .set_input(GateId(2), Signal::from_u64(1, 1))
        .unwrap();
    assert_eq!(backend.value(NetId(4)).unwrap().to_u64(), Some(10));
    backend
        .set_input(GateId(3), Signal::from_u64(1, 1))
        .unwrap();
    assert_eq!(
        backend.value(NetId(4)).unwrap(),
        &Signal::filled(4, Logic::Unknown)
    );
    backend.advance(20).unwrap();
    assert_eq!(backend.value(NetId(1)).unwrap().to_u64(), Some(4));
    assert_eq!(backend.value(NetId(5)).unwrap().to_u64(), Some(0));
    assert!(
        backend
            .value(NetId(6))
            .unwrap()
            .bits()
            .chunks(4)
            .all(|bits| bits == [Logic::HighZ, Logic::Unknown, Logic::Low, Logic::High])
    );
    let wide = Signal::filled(128, Logic::HighZ);
    backend.set_input(GateId(6), wide.clone()).unwrap();
    assert_eq!(backend.value(NetId(6)).unwrap(), &wide);
    assert_eq!(backend.traces()[&NetId(4)].transitions.len(), 3);
}
#[test]
fn exact_subnanosecond_steps_and_invalid_inputs() {
    let source = "`timescale 1ns/100ps\nmodule top;reg clk=0; always #0.5 clk=~clk; endmodule";
    let mut backend =
        VerilogBackend::new(vec![source.into()], "top", vec![binding(1, "clk", 1, true)]).unwrap();
    backend.step().unwrap();
    assert_eq!(backend.time_ticks(), 5);
    assert_eq!(backend.ticks_per_nanosecond(), 10);
    assert_eq!(backend.value(NetId(1)).unwrap().to_u64(), Some(1));
    backend.step().unwrap();
    assert_eq!(backend.time_ticks(), 10);
    assert_eq!(backend.time(), 1);
    assert!(
        backend
            .set_input(GateId(1), Signal::from_u64(0, 2))
            .is_err()
    );
    assert!(backend.probe(NetId(99)).is_err());
    assert!(backend.advance(u64::MAX).is_err());
    assert!(
        VerilogBackend::new(
            vec![source.into()],
            "top",
            vec![binding(1, "missing", 1, false)]
        )
        .is_err()
    );
}

#[test]
fn binds_internal_signals_without_resetting_runtime() {
    let source = "`timescale 1ns/1ps\nmodule child(input clk);reg [7:0] count=0;always @(posedge clk)count<=count+1;endmodule\nmodule top;reg clk=0;always #5 clk=~clk;child u(clk);endmodule";
    let mut backend = VerilogBackend::new(vec![source.into()], "top", vec![]).unwrap();
    backend.advance(16).unwrap();
    let before = backend.time_ticks();
    let (net, _) = backend.bind_discovered("top.u.count", false).unwrap();
    assert_eq!(backend.time_ticks(), before);
    assert_eq!(backend.value(net).unwrap().to_u64(), Some(2));
    backend.probe(net).unwrap();
    backend.advance(10).unwrap();
    assert_eq!(backend.value(net).unwrap().to_u64(), Some(3));
    assert_eq!(
        backend.bind_discovered("top.u.count", false).unwrap().0,
        net
    );
    assert!(backend.bind_discovered("top.missing", false).is_err());
}

#[test]
fn cancelled_compilation_does_not_launch_runtime() {
    let token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    assert!(
        VerilogBackend::new_cancellable(vec!["module top;endmodule".into()], "top", vec![], &token)
            .err()
            .unwrap()
            .to_string()
            .contains("cancelled")
    );
}

#[test]
fn force_release_and_hierarchical_refs_work() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/verilog");
    let pwm_rtl = std::fs::read_to_string(directory.join("projectf/pwm.sv")).unwrap();
    let harness = std::fs::read_to_string(directory.join("harnesses/pwm-rgate.sv")).unwrap();
    let mut backend = VerilogBackend::new(
        vec![pwm_rtl, harness],
        "pwm_rgate",
        vec![
            binding(1, "RESET_N", 1, true),
            binding(2, "duty", 8, true),
            binding(3, "count", 8, false),
            binding(4, "led", 1, false),
        ],
    )
    .unwrap();
    // While RESET_N == 0, dimmer.cnt is forced to 0
    backend.advance(300).unwrap();
    assert_eq!(backend.value(NetId(3)).unwrap().to_u64(), Some(0));
    // Release reset: counter starts incrementing on rising edges of CLK (period 100ns)
    backend
        .set_input(GateId(1), Signal::from_u64(1, 1))
        .unwrap();
    backend.advance(500).unwrap();
    assert_eq!(backend.value(NetId(3)).unwrap().to_u64(), Some(5));
    assert_eq!(backend.value(NetId(4)).unwrap().to_u64(), Some(1));
}

#[test]
fn lc3_hdl_and_led_chaser_rgate_examples_run() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for file in ["lc3-hdl.rgate", "verilog-led-chaser.rgate", "pwm-verilog.rgate"] {
        let content = std::fs::read_to_string(root.join(file)).unwrap();
        let circuit = rgate_format::native::decode(&content).unwrap();
        let exported = rgate_format::export_interactive(&circuit).unwrap();
        let mut backend = VerilogBackend::new(vec![exported], &circuit.root, vec![]).unwrap();
        backend.advance(200).unwrap();
    }
}

#[test]
fn functions_generate_indexed_part_select_and_diagnostics() {
    let source = r#"
`timescale 1ns/1ps
module top;
    parameter N = 4;
    reg [15:0] data = 16'hA53C;
    wire [3:0] nibble [0:N-1];
    reg [3:0] sel = 0;
    reg [3:0] out_nibble;
    wire [7:0] rev_byte;

    function [7:0] reverse8(input [7:0] v);
        integer i;
        begin
            for (i = 0; i < 8; i = i + 1)
                reverse8[i] = v[7 - i];
        end
    endfunction

    assign rev_byte = reverse8(data[7:0]);

    generate
        genvar g;
        for (g = 0; g < N; g = g + 1) begin : gen_nib
            wire [3:0] slice = data[g*4 +: 4];
        end
    endgenerate

    always @* begin
        out_nibble = data[sel*4 +: 4];
    end
endmodule
"#;
    let mut backend = VerilogBackend::new(
        vec![source.into()],
        "top",
        vec![
            binding(1, "sel", 4, true),
            binding(2, "out_nibble", 4, false),
            binding(3, "rev_byte", 8, false),
        ],
    )
    .unwrap();
    assert_eq!(backend.value(NetId(2)).unwrap().to_u64(), Some(0xC));
    // 0x3C = 0011_1100 -> reversed = 0011_1100 = 0x3C
    assert_eq!(backend.value(NetId(3)).unwrap().to_u64(), Some(0x3C));
    backend
        .set_input(GateId(1), Signal::from_u64(3, 4))
        .unwrap();
    assert_eq!(backend.value(NetId(2)).unwrap().to_u64(), Some(0xA));

    // Check diagnostics formatting `<module>.sv:<line>: ...`
    let err = rgate_hdl::check_sources(&[(
        "broken".into(),
        "module broken;\n  wire a = ;\nendmodule\n".into(),
    )])
    .unwrap_err()
    .to_string();
    assert!(err.contains("broken.sv:2:"), "unexpected diagnostic: {err}");
}

#[test]
fn zero_delay_combinational_loop_fails_cleanly() {
    let source = "module top; reg a = 0; always @(a) a = ~a; initial a = 1; endmodule";
    let err = VerilogBackend::new(vec![source.into()], "top", vec![])
        .err()
        .expect("should error on zero-delay loop")
        .to_string();
    assert!(err.contains("delta limit"), "unexpected error: {err}");
}
