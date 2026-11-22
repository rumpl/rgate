use crate::wiring::{endpoint_positions, update_wire_endpoints};
use crate::*;
use rgate_core::{Direction, Net, Signal, demo};
use std::collections::BTreeSet;

fn editor() -> Editor {
    Editor::new(demo::full_adder()).unwrap()
}

#[test]
fn swapping_inputs_preserves_nets_wire_ends_and_undo() {
    for rotation in 0..4 {
        let mut editor = editor();
        let id = editor
            .module()
            .gates
            .iter()
            .find(|gate| gate.kind == GateKind::Or)
            .unwrap()
            .id;
        editor
            .edit_module(|module| {
                let old = endpoint_positions(module, &BTreeSet::from([id]));
                module.gate_mut(id).unwrap().rotation = rotation;
                update_wire_endpoints(module, &old, &BTreeSet::from([id]));
                // Exercise references on either end of a wire.
                for wire in &mut module.wires {
                    if wire
                        .end
                        .as_ref()
                        .is_some_and(|pin| pin.gate == id && pin.pin == "I0")
                    {
                        std::mem::swap(&mut wire.start, &mut wire.end);
                        wire.points.reverse();
                    }
                }
            })
            .unwrap();
        let before = editor.circuit().clone();
        let gate = editor.module().gate(id).unwrap();
        let (a, b) = (gate.pin("I0").unwrap().net, gate.pin("I1").unwrap().net);
        editor.swap_inputs(id, "I0", "I1").unwrap();
        let gate = editor.module().gate(id).unwrap();
        assert_eq!(gate.pin("I0").unwrap().net, b);
        assert_eq!(gate.pin("I1").unwrap().net, a);
        assert_eq!(editor.module().nets, before.modules[0].nets);
        for wire in &editor.module().wires {
            for (reference, point) in [
                (&wire.start, wire.points.first()),
                (&wire.end, wire.points.last()),
            ] {
                if let Some(reference) = reference {
                    assert_eq!(editor.module().pin_position(reference), point.copied());
                }
            }
        }
        editor.circuit().validate().unwrap();
        let swapped = editor.circuit().clone();
        editor.undo();
        assert_eq!(editor.circuit(), &before);
        editor.redo();
        assert_eq!(editor.circuit(), &swapped);
    }
}

#[test]
fn swapping_selected_pair_leaves_other_inputs_and_checks_widths() {
    let mut editor = Editor::new(Circuit::default()).unwrap();
    let id = editor.place(GateKind::And, Point::ZERO).unwrap();
    editor
        .edit_module(|module| {
            let gate = module.gate_mut(id).unwrap();
            gate.input_count = 4;
            gate.reset_pins();
            for (index, pin) in gate.pins.iter_mut().take(4).enumerate() {
                let net = NetId(index as u64 + 1);
                pin.net = Some(net);
            }
            for index in 0..4 {
                module
                    .nets
                    .push(Net::new(NetId(index + 1), format!("input{index}"), 1));
            }
        })
        .unwrap();
    editor.swap_inputs(id, "I0", "I3").unwrap();
    let gate = editor.module().gate(id).unwrap();
    for (pin, net) in [("I0", 4), ("I1", 2), ("I2", 3), ("I3", 1)] {
        assert_eq!(gate.pin(pin).unwrap().net, Some(NetId(net)));
    }
    let before = editor.circuit().clone();
    assert!(editor.swap_inputs(id, "I0", "I0").is_err());
    assert!(editor.swap_inputs(id, "I0", "Z").is_err());
    assert!(editor.swap_inputs(id, "I0", "missing").is_err());
    assert_eq!(editor.circuit(), &before);
    editor
        .edit_module(|module| module.gate_mut(id).unwrap().pin_mut("I3").unwrap().width = Some(8))
        .unwrap();
    let before = editor.circuit().clone();
    assert!(matches!(
        editor.swap_inputs(id, "I0", "I3"),
        Err(EditError::WidthMismatch(1, 8))
    ));
    assert_eq!(editor.circuit(), &before);
}

#[test]
fn swapping_connected_and_unconnected_inputs_keeps_wire_on_its_net() {
    let mut editor = Editor::new(Circuit::default()).unwrap();
    let source = editor
        .place(GateKind::Switch, Point::new(20., 30.))
        .unwrap();
    let target = editor.place(GateKind::Or, Point::new(100., 30.)).unwrap();
    editor
        .start_wire(Hit::Pin(PinRef::new(source, "Z")))
        .unwrap();
    let net = editor
        .finish_wire(Hit::Pin(PinRef::new(target, "I0")))
        .unwrap();
    editor.swap_inputs(target, "I0", "I1").unwrap();
    assert_eq!(
        editor.module().pin(&PinRef::new(target, "I0")).unwrap().net,
        None
    );
    assert_eq!(
        editor.module().pin(&PinRef::new(target, "I1")).unwrap().net,
        Some(net)
    );
    let wire = &editor.module().wires[0];
    assert_eq!(wire.net, net);
    assert_eq!(wire.end, Some(PinRef::new(target, "I1")));
    assert_eq!(
        wire.points.last().copied(),
        editor.module().pin_position(&PinRef::new(target, "I1"))
    );
}

#[test]
fn gesture_is_one_undo_step_and_dirty_tracks_saved_content() {
    let mut editor = editor();
    let initial = editor.circuit().clone();
    editor.select(GateId(1), false);
    editor.begin_gesture();
    for _ in 0..10 {
        editor.move_selection(Point::new(5.0, 0.0));
    }
    editor.finish_gesture();
    editor.circuit().validate().unwrap();
    assert!(editor.is_dirty());
    editor.undo();
    assert_eq!(editor.circuit(), &initial);
    assert!(!editor.is_dirty());
    assert!(!editor.can_undo());
    editor.redo();
    assert!(editor.is_dirty());
}

#[test]
fn paste_remaps_ids_nets_names_and_endpoints() {
    let mut editor = editor();
    editor.select_all();
    let copied = editor.copy_selection();
    let count = copied.gates.len();
    editor.paste(&copied, Point::new(40.0, 30.0)).unwrap();
    assert_eq!(editor.module().gates.len(), count * 2);
    editor.circuit().validate().unwrap();
}

#[test]
fn wiring_connects_pins_and_merges_nets() {
    let mut editor = Editor::new(Circuit::default()).unwrap();
    let a = editor
        .place(GateKind::Switch, Point::new(20.0, 30.0))
        .unwrap();
    let b = editor.place(GateKind::Not, Point::new(80.0, 30.0)).unwrap();
    editor.start_wire(Hit::Pin(PinRef::new(a, "Z"))).unwrap();
    let net = editor.finish_wire(Hit::Pin(PinRef::new(b, "I"))).unwrap();
    assert_eq!(
        editor.module().pin(&PinRef::new(a, "Z")).unwrap().net,
        Some(net)
    );
    assert_eq!(
        editor.module().pin(&PinRef::new(b, "I")).unwrap().net,
        Some(net)
    );
    editor.circuit().validate().unwrap();
    editor.undo();
    assert!(editor.module().wires.is_empty());
}

#[test]
fn mismatched_bus_width_is_rejected_without_mutation() {
    let mut editor = Editor::new(Circuit::default()).unwrap();
    let a = editor.place(GateKind::Dip, Point::ZERO).unwrap();
    let b = editor.place(GateKind::Not, Point::new(80.0, 0.0)).unwrap();
    editor.start_wire(Hit::Pin(PinRef::new(a, "Z"))).unwrap();
    let before = editor.circuit().clone();
    assert!(matches!(
        editor.finish_wire(Hit::Pin(PinRef::new(b, "I"))),
        Err(EditError::WidthMismatch(8, 1))
    ));
    assert_eq!(editor.circuit(), &before);
}

#[test]
fn invalid_property_edit_rolls_back() {
    let mut editor = editor();
    let before = editor.circuit().clone();
    assert!(
        editor
            .edit_module(|module| module.gates[0].width = 0)
            .is_err()
    );
    assert_eq!(editor.circuit(), &before);
}

#[test]
fn zoom_keeps_cursor_anchored() {
    let mut viewport = Viewport::default();
    let anchor = Point::new(250.0, 180.0);
    let world = viewport.to_world(anchor);
    viewport.zoom_at(1.25, anchor);
    assert!(viewport.to_world(anchor).distance(world) < 0.001);
}

#[test]
fn deletion_leaves_valid_dangling_wires() {
    let mut editor = editor();
    editor.select(GateId(1), false);
    editor.delete_selection().unwrap();
    editor.circuit().validate().unwrap();
    assert!(editor.module().gate(GateId(1)).is_none());
}

#[test]
fn module_creation_is_undoable_and_switching_preserves_contents() {
    let mut editor = editor();
    let original = editor.circuit().clone();
    editor.create_module("child".into()).unwrap();
    assert_eq!(editor.active_module(), "child");
    assert!(editor.is_dirty());
    assert!(editor.create_module("child".into()).is_err());
    editor.undo();
    assert_eq!(editor.circuit(), &original);
    assert_eq!(editor.active_module(), "main");
    editor.redo();
    editor.switch_module("child").unwrap();
    editor.place(GateKind::And, Point::ZERO).unwrap();
    editor.switch_module("main").unwrap();
    assert_eq!(editor.module().gates.len(), 12);
    editor.switch_module("child").unwrap();
    assert_eq!(editor.module().gates.len(), 1);
}

#[test]
fn placing_instances_and_interface_updates_are_transactional() {
    let mut editor = Editor::new(demo::hierarchical_inverters()).unwrap();
    let id = editor
        .place(
            GateKind::Module("inverter".into()),
            Point::new(500.0, 200.0),
        )
        .unwrap();
    assert_eq!(editor.module().gate(id).unwrap().pins.len(), 2);
    let before = editor.circuit().clone();
    assert!(
        editor
            .place(GateKind::Module("main".into()), Point::ZERO)
            .is_err()
    );
    assert_eq!(editor.circuit(), &before);
    editor.switch_module("inverter").unwrap();
    editor
        .edit_module(|module| {
            let mut bus = Net::new(NetId(3), "bus", 8);
            bus.port = Some(rgate_core::Direction::Input);
            module.nets.push(bus);
        })
        .unwrap();
    editor.switch_module("main").unwrap();
    assert_eq!(editor.module().gate(id).unwrap().pin_width("bus"), 8);
    editor.switch_module("inverter").unwrap();
    let before = editor.circuit().clone();
    assert!(
        editor
            .edit_module(|module| module.nets[0].width = 8)
            .is_err()
    );
    assert_eq!(editor.circuit(), &before);
}

#[test]
fn wire_drag_is_snapped_endpoint_preserving_and_one_undo_step() {
    let mut editor = Editor::new(demo::bus_memory()).unwrap();
    let original = editor.circuit().clone();
    let wire = editor.module().wires[1].clone();
    let anchor = (wire.points[0] + wire.points[1]) / 2.0;
    let drag = editor.begin_wire_drag(wire.id, anchor).unwrap();
    for offset in [3.0, 8.0, 12.0] {
        editor.drag_wire_segment(&drag, anchor + Point::new(20.0, offset));
    }
    let changed = editor
        .module()
        .wires
        .iter()
        .find(|other| other.id == wire.id)
        .unwrap();
    assert_eq!(changed.points[0], wire.points[0]);
    assert_eq!(changed.points.last(), wire.points.last());
    assert_eq!(changed.start, wire.start);
    assert_eq!(changed.end, wire.end);
    assert_eq!(changed.net, wire.net);
    assert!(
        changed
            .points
            .windows(2)
            .all(|segment| segment[0].x == segment[1].x || segment[0].y == segment[1].y)
    );
    editor.circuit().validate().unwrap();
    editor.finish_gesture();
    editor.undo();
    assert_eq!(editor.circuit(), &original);
    assert!(!editor.can_undo());
    editor.redo();
    assert_ne!(editor.circuit(), &original);
}

#[test]
fn dragging_vertical_segment_can_be_cancelled_without_editing_connectivity() {
    let mut editor = editor();
    let original = editor.circuit().clone();
    let wire = editor.module().wires[0].clone();
    let anchor = (wire.points[1] + wire.points[2]) / 2.0;
    let drag = editor.begin_wire_drag(wire.id, anchor).unwrap();
    editor.drag_wire_segment(&drag, anchor + Point::new(25.0, 0.0));
    let changed = &editor.module().wires[0];
    assert_eq!(changed.points[1].x, wire.points[1].x + 25.0);
    assert_eq!(changed.points[0], wire.points[0]);
    assert_eq!(changed.points.last(), wire.points.last());
    editor.cancel();
    assert_eq!(editor.circuit(), &original);
    assert!(!editor.can_undo());
}

#[test]
fn dropping_narrow_wire_on_bus_creates_undoable_tap() {
    let mut editor = Editor::new(demo::bus_memory()).unwrap();
    let input = editor
        .place(GateKind::Led, Point::new(200.0, 200.0))
        .unwrap();
    let bus = editor.module().wires[0].clone();
    let point = bus.points[1];
    let original = editor.circuit().clone();
    editor
        .start_wire(Hit::Pin(PinRef::new(input, "I")))
        .unwrap();
    let net = editor
        .finish_wire(Hit::Wire {
            id: bus.id,
            net: bus.net,
            point,
        })
        .unwrap();
    let tap = editor
        .module()
        .gates
        .iter()
        .find(|gate| gate.kind == GateKind::Tap)
        .unwrap();
    assert_eq!(tap.config.tap_width, 1);
    assert_eq!(tap.pin("I").unwrap().net, Some(bus.net));
    assert_eq!(tap.pin("Z").unwrap().net, Some(net));
    editor.circuit().validate().unwrap();
    editor.undo();
    assert_eq!(editor.circuit(), &original);
}

#[test]
fn custom_module_symbol_propagates_and_undo_restores_instances() {
    let mut editor = Editor::new(demo::hierarchical_inverters()).unwrap();
    let shape = rgate_core::SymbolPrimitive::Ellipse {
        start: Point::new(-40.0, -30.0),
        end: Point::new(40.0, 30.0),
        filled: false,
    };
    editor
        .set_module_symbol("wrapped_inverter", vec![shape.clone()])
        .unwrap();
    assert_eq!(
        editor
            .circuit()
            .module("main")
            .unwrap()
            .gate(GateId(2))
            .unwrap()
            .config
            .custom_symbol,
        vec![shape]
    );
    editor.undo();
    assert!(
        editor
            .circuit()
            .module("main")
            .unwrap()
            .gate(GateId(2))
            .unwrap()
            .config
            .custom_symbol
            .is_empty()
    );
}

#[test]
fn removing_both_gates_cleans_wires_simultaneously_or_separately() {
    for together in [true, false] {
        let mut editor = Editor::new(Circuit::default()).unwrap();
        let a = editor.place(GateKind::Switch, Point::ZERO).unwrap();
        let b = editor.place(GateKind::Led, Point::new(100.0, 0.0)).unwrap();
        editor.start_wire(Hit::Pin(PinRef::new(a, "Z"))).unwrap();
        editor.finish_wire(Hit::Pin(PinRef::new(b, "I"))).unwrap();
        let before = editor.circuit().clone();
        editor.select(a, false);
        if together {
            editor.select(b, true);
        }
        editor.delete_selection().unwrap();
        if !together {
            assert_eq!(editor.module().wires.len(), 1);
            editor.select(b, false);
            editor.delete_selection().unwrap();
        }
        assert!(editor.module().wires.is_empty());
        assert!(editor.module().nets.is_empty());
        editor.circuit().validate().unwrap();
        editor.undo();
        if !together {
            editor.undo();
        }
        assert_eq!(editor.circuit(), &before);
    }
}

#[test]
fn wire_click_shift_toggle_and_region_select_work_independently_of_gates() {
    let mut editor = editor();
    editor.select_wire(WireId(1), false);
    assert_eq!(editor.selection.wires, BTreeSet::from([WireId(1)]));
    editor.select_wire(WireId(2), true);
    assert_eq!(editor.selection.wires.len(), 2);
    editor.select_wire(WireId(1), true);
    assert_eq!(editor.selection.wires, BTreeSet::from([WireId(2)]));
    editor.select_region(
        Rect::from_points(Point::new(160.0, 123.0), Point::new(175.0, 131.0)),
        false,
    );
    assert_eq!(editor.selection.wires, BTreeSet::from([WireId(1)]));
    assert!(editor.selection.gates.is_empty());
}

mod port_marker_tests {
    use super::*;

    #[test]
    fn port_markers_create_named_nets_and_instance_pins_without_sources() {
        let mut editor = Editor::new(Circuit::default()).unwrap();
        editor.create_module("and_block".into()).unwrap();
        let and = editor
            .place(GateKind::And, Point::new(100.0, 100.0))
            .unwrap();
        let a = editor
            .place(GateKind::InputPort, Point::new(10.0, 70.0))
            .unwrap();
        let b = editor
            .place(GateKind::InputPort, Point::new(10.0, 130.0))
            .unwrap();
        let y = editor
            .place(GateKind::OutputPort, Point::new(200.0, 100.0))
            .unwrap();
        editor
            .edit_module(|m| {
                m.gate_mut(a).unwrap().name = "A".into();
                m.gate_mut(b).unwrap().name = "B".into();
                m.gate_mut(y).unwrap().name = "Y".into();
            })
            .unwrap();
        for (marker, pin) in [(a, "I0"), (b, "I1"), (y, "Z")] {
            editor.start_wire(Hit::Pin(PinRef::new(and, pin))).unwrap();
            editor
                .finish_wire(Hit::Pin(PinRef::new(marker, "P")))
                .unwrap();
        }
        let nets = &editor.module().nets;
        assert_eq!(nets.len(), 3);
        assert!(nets.iter().all(|n| n.port.is_some()));
        let instance = editor
            .circuit()
            .module_instance("and_block", GateId(99), Point::ZERO)
            .unwrap();
        assert_eq!(instance.pins.len(), 3);
        assert_eq!(instance.pin("A").unwrap().direction, Direction::Input);
        assert_eq!(instance.pin("Y").unwrap().direction, Direction::Output);
        assert!(!editor.module().gate(a).unwrap().kind.is_source());
        editor.circuit().validate().unwrap();
    }

    #[test]
    fn inout_marker_width_and_name_are_undoable() {
        let mut editor = Editor::new(Circuit::default()).unwrap();
        let id = editor.place(GateKind::InoutPort, Point::ZERO).unwrap();
        let before = editor.circuit().clone();
        editor
            .edit_module(|m| {
                let g = m.gate_mut(id).unwrap();
                g.name = "DATA".into();
                g.width = 8;
                g.initial = Signal::from_u64(0, 8);
                g.reset_pins();
                g.pin_mut("P").unwrap().net = Some(NetId(1));
            })
            .unwrap();
        assert_eq!(editor.module().nets[0].name, "DATA");
        assert_eq!(editor.module().nets[0].width, 8);
        assert_eq!(editor.module().nets[0].port, Some(Direction::InOut));
        editor.undo();
        assert_eq!(editor.circuit(), &before);
    }
}

mod port_rename_tests {
    use super::*;

    #[test]
    fn net_and_port_marker_can_be_renamed_from_either_properties() {
        let mut editor = Editor::new(Circuit::default()).unwrap();
        let marker = editor.place(GateKind::InputPort, Point::ZERO).unwrap();
        let net = editor
            .module()
            .gate(marker)
            .unwrap()
            .pin("P")
            .unwrap()
            .net
            .unwrap();
        let before = editor.circuit().clone();
        editor
            .edit_module(|m| m.nets.iter_mut().find(|n| n.id == net).unwrap().name = "RESET".into())
            .unwrap();
        assert_eq!(editor.module().net(net).unwrap().name, "RESET");
        assert_eq!(editor.module().gate(marker).unwrap().name, "RESET");
        editor
            .edit_module(|m| m.gate_mut(marker).unwrap().name = "RESET_N".into())
            .unwrap();
        assert_eq!(editor.module().net(net).unwrap().name, "RESET_N");
        editor.undo();
        assert_eq!(editor.module().net(net).unwrap().name, "RESET");
        editor.undo();
        assert_eq!(editor.circuit(), &before);
    }
}
