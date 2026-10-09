#![cfg(feature = "icarus")]
use rgate_core::{GateId, Logic, NetId, Signal};
use rgate_hdl::{HdlProject, HdlSignal, IcarusBackend};
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
fn downloaded_designs_run_without_replay() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/verilog");
    for name in ["projectf", "uart", "picorv32", "four-state", "pwm"] {
        let project = HdlProject::load(&directory.join(format!("{name}.json"))).unwrap();
        let mut backend = project.instantiate_icarus(&directory).unwrap();
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
    let mut backend = project.instantiate_icarus(&directory).unwrap();
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
    let mut backend = IcarusBackend::new(
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
        IcarusBackend::new(vec![source.into()], "top", vec![binding(1, "clk", 1, true)]).unwrap();
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
        IcarusBackend::new(
            vec![source.into()],
            "top",
            vec![binding(1, "missing", 1, false)]
        )
        .is_err()
    );
}
