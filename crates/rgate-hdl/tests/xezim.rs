#![cfg(feature = "xezim")]
use rgate_core::{GateId, Logic, NetId, Signal};
use rgate_hdl::{HdlSignal, XezimBackend};
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
fn interactive_hierarchy_and_four_state() {
    let source = r#"`timescale 1ns/1ps
 module counter(input clk, reset, enable, output reg [7:0] count);
 always @(posedge clk) if(reset) count<=0; else if(enable) count<=count+1;
 endmodule
 module top;
 reg clk=0; always #5 clk=~clk;
 reg reset=1, enable=0;
 wire [7:0] count; counter u(clk,reset,enable,count);
 reg [3:0] drive=4'b10xz; wire [3:0] bus=drive;
 endmodule"#;
    let mut sim = XezimBackend::new(
        vec![source.into()],
        "top",
        vec![
            binding(1, "reset", 1, true),
            binding(2, "enable", 1, true),
            binding(3, "u.count", 8, false),
            binding(4, "drive", 4, true),
            binding(5, "bus", 4, false),
        ],
    )
    .unwrap();
    assert_eq!(
        sim.value(NetId(3)).unwrap(),
        &Signal::filled(8, Logic::Unknown)
    );
    assert_eq!(
        sim.value(NetId(5)).unwrap().bits(),
        &[Logic::HighZ, Logic::Unknown, Logic::Low, Logic::High]
    );
    sim.probe(NetId(3)).unwrap();
    sim.advance(6).unwrap();
    assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(0));
    sim.set_input(GateId(1), Signal::from_u64(0, 1)).unwrap();
    sim.set_input(GateId(2), Signal::from_u64(1, 1)).unwrap();
    sim.advance(10).unwrap();
    assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(1));
    sim.step().unwrap();
    assert_eq!(sim.time(), 17);
    assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(1));
    sim.advance(10).unwrap();
    assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(2));
    sim.set_input(GateId(2), Signal::from_u64(0, 1)).unwrap();
    sim.advance(20).unwrap();
    assert_eq!(sim.value(NetId(3)).unwrap().to_u64(), Some(2));
    let trace = &sim.traces()[&NetId(3)];
    assert!(
        trace
            .transitions
            .iter()
            .any(|t| t.time == 15 && t.value.to_u64() == Some(1)),
        "{trace:?}"
    );
    sim.set_input(
        GateId(4),
        Signal::from_bits(vec![Logic::Low, Logic::HighZ, Logic::Unknown, Logic::High]),
    )
    .unwrap();
    assert_eq!(
        sim.value(NetId(5)).unwrap().bits(),
        &[Logic::Low, Logic::HighZ, Logic::Unknown, Logic::High]
    );
}

#[test]
fn downloaded_designs_execute_and_vga_sync_has_exact_periods() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/verilog");
    for name in ["projectf", "uart", "picorv32", "four-state", "pwm"] {
        let project = rgate_hdl::HdlProject::load(&directory.join(format!("{name}.json"))).unwrap();
        let mut backend = project.instantiate(&directory).unwrap();
        project.execute(&mut backend).unwrap();
        if name == "projectf" {
            let hsync = backend
                .signals()
                .iter()
                .find(|s| s.path == "hsync")
                .unwrap()
                .net;
            let trace = &backend.traces()[&hsync];
            let lows = trace
                .transitions
                .iter()
                .filter(|t| t.value.to_u64() == Some(0))
                .collect::<Vec<_>>();
            assert!(lows.len() > 25);
            assert_eq!(lows[1].time - lows[0].time, 480);
            let first = trace
                .transitions
                .iter()
                .position(|t| t.value.to_u64() == Some(0))
                .unwrap();
            assert_eq!(
                trace.transitions[first + 1].time - trace.transitions[first].time,
                40
            );
            let vsync = backend
                .signals()
                .iter()
                .find(|s| s.path == "vsync")
                .unwrap()
                .net;
            let trace = &backend.traces()[&vsync];
            let first = trace
                .transitions
                .iter()
                .position(|t| t.value.to_u64() == Some(0))
                .unwrap();
            assert_eq!(
                trace.transitions[first + 1].time - trace.transitions[first].time,
                960
            );
        }
        let vcd = rgate_sim::export_vcd(
            &backend.traces().values().cloned().collect::<Vec<_>>(),
            backend.time(),
        )
        .unwrap();
        assert!(vcd.contains("$timescale 1ns $end"));
    }
}

#[test]
fn resolved_tristates_delays_wide_vectors_and_user_monitor() {
    let source = r#"`timescale 1ns/1ps
    module top;
      reg enable=0;
      reg conflicting=0;
      wire [3:0] bus;
      assign bus=enable ? 4'b1010 : 4'bzzzz;
      assign bus=conflicting ? 4'b0101 : 4'bzzzz;
      wire [3:0] delayed;
      assign #2 delayed=bus;
      wire [127:0] wide={32{bus}};
      initial $monitor("USER_MONITOR %b",bus);
    endmodule"#;
    let mut backend = XezimBackend::new(
        vec![source.into()],
        "top",
        vec![
            binding(1, "enable", 1, true),
            binding(2, "conflicting", 1, true),
            binding(3, "bus", 4, false),
            binding(4, "delayed", 4, false),
            binding(5, "wide", 128, false),
        ],
    )
    .unwrap();
    backend.probe(NetId(3)).unwrap();
    backend.probe(NetId(4)).unwrap();
    assert_eq!(
        backend.value(NetId(3)).unwrap(),
        &Signal::filled(4, Logic::HighZ)
    );
    backend.advance(3).unwrap();
    backend
        .set_input(GateId(1), Signal::from_u64(1, 1))
        .unwrap();
    assert_eq!(backend.value(NetId(3)).unwrap().to_u64(), Some(10));
    assert_eq!(
        backend.value(NetId(4)).unwrap(),
        &Signal::filled(4, Logic::HighZ)
    );
    backend.advance(2).unwrap();
    assert_eq!(backend.value(NetId(4)).unwrap().to_u64(), Some(10));
    assert_eq!(backend.value(NetId(5)).unwrap().width(), 128);
    assert!(
        backend
            .value(NetId(5))
            .unwrap()
            .bits()
            .chunks(4)
            .all(|bits| bits == [Logic::Low, Logic::High, Logic::Low, Logic::High])
    );
    backend
        .set_input(GateId(2), Signal::from_u64(1, 1))
        .unwrap();
    assert_eq!(
        backend.value(NetId(3)).unwrap(),
        &Signal::filled(4, Logic::Unknown)
    );
    assert!(
        backend.traces()[&NetId(3)]
            .transitions
            .iter()
            .any(|t| t.time == 3 && t.value.to_u64() == Some(10))
    );
    assert!(
        backend.traces()[&NetId(4)]
            .transitions
            .iter()
            .any(|t| t.time == 5 && t.value.to_u64() == Some(10))
    );
}

#[test]
fn precise_timescale_and_partitioned_advances_match_single_run() {
    let source = "`timescale 1ns/100ps\nmodule top; reg clk=0; always #0.5 clk=~clk; reg [7:0] count=0; always @(posedge clk) count<=count+1; endmodule";
    let make = || {
        XezimBackend::new(
            vec![source.into()],
            "top",
            vec![binding(1, "count", 8, false)],
        )
        .unwrap()
    };
    let mut whole = make();
    whole.probe(NetId(1)).unwrap();
    whole.advance(20).unwrap();
    let mut segmented = make();
    segmented.probe(NetId(1)).unwrap();
    for _ in 0..20 {
        segmented.step().unwrap();
    }
    assert_eq!(whole.value(NetId(1)), segmented.value(NetId(1)));
    assert_eq!(whole.value(NetId(1)).unwrap().to_u64(), Some(20));
    assert_eq!(
        whole.traces()[&NetId(1)].transitions,
        segmented.traces()[&NetId(1)].transitions
    );
}

#[test]
fn invalid_bindings_and_failed_operations_are_not_committed() {
    let source = "module top; reg [1:0] a=0; endmodule";
    assert!(
        XezimBackend::new(
            vec![source.into()],
            "top",
            vec![binding(1, "missing", 1, false)]
        )
        .is_err()
    );
    assert!(
        XezimBackend::new(vec![source.into()], "top", vec![binding(1, "a", 1, false)]).is_err()
    );
    assert!(
        XezimBackend::new(
            vec![source.into()],
            "top",
            vec![binding(1, "a; $finish", 2, false)]
        )
        .is_err()
    );
    let mut finished = XezimBackend::new(
        vec!["`timescale 1ns/1ps\nmodule top; initial #1 $finish; endmodule".into()],
        "top",
        vec![],
    )
    .unwrap();
    assert!(finished.advance(2).is_err());
    let mut backend =
        XezimBackend::new(vec![source.into()], "top", vec![binding(1, "a", 2, true)]).unwrap();
    assert!(
        backend
            .set_input(GateId(1), Signal::from_u64(1, 1))
            .is_err()
    );
    assert!(backend.advance(1_000_001).is_err());
    assert_eq!(backend.time(), 0);
    assert_eq!(backend.value(NetId(1)).unwrap().to_u64(), Some(0));
    assert!(backend.probe(NetId(99)).is_err());
    assert!(backend.traces().is_empty());
    let source = "`timescale 1ns/1ps\nmodule top; reg reset=0; always @* if(reset) $fatal(1,\"bad\"); endmodule";
    let mut backend = XezimBackend::new(
        vec![source.into()],
        "top",
        vec![binding(1, "reset", 1, true)],
    )
    .unwrap();
    backend.advance(1).unwrap();
    assert!(
        backend
            .set_input(GateId(1), Signal::from_u64(1, 1))
            .is_err()
    );
    backend.advance(1).unwrap();
    assert_eq!(backend.value(NetId(1)).unwrap().to_u64(), Some(0));
}
