//! VCD export of retained trace history. Unknown and high-impedance bits stay X/Z.
use crate::Trace;
use std::{collections::BTreeMap, fmt::Write};

pub fn export_vcd(traces: &[Trace], end_time: u64) -> Result<String, std::fmt::Error> {
    let mut output =
        String::from("$version RGate $end\n$timescale 1ns $end\n$scope module rgate $end\n");
    for (index, trace) in traces.iter().enumerate() {
        // Keep full hierarchy in an escaped identifier; slash is legal inside it.
        let name = trace
            .name
            .chars()
            .map(|ch| if ch.is_whitespace() { '_' } else { ch })
            .collect::<String>();
        writeln!(output, "$var wire {} n{index} \\{name} $end", trace.width)?;
    }
    output.push_str("$upscope $end\n$enddefinitions $end\n");
    let mut times: BTreeMap<u64, Vec<(usize, &crate::Transition)>> = BTreeMap::new();
    for (index, trace) in traces.iter().enumerate() {
        for transition in &trace.transitions {
            times
                .entry(transition.time)
                .or_default()
                .push((index, transition));
        }
    }
    for (time, changes) in times {
        writeln!(output, "#{time}")?;
        for (index, change) in changes {
            let bits = change
                .value
                .bits()
                .iter()
                .rev()
                .map(|bit| bit.to_string().to_lowercase())
                .collect::<String>();
            if change.value.width() == 1 {
                writeln!(output, "{bits}n{index}")?;
            } else {
                writeln!(output, "b{bits} n{index}")?;
            }
        }
    }
    writeln!(output, "#{end_time}")?;
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Transition;
    use rgate_core::{Logic, NetId, Signal};
    #[test]
    fn vcd_preserves_width_time_and_four_state_values() {
        let traces = vec![Trace {
            net: NetId(1),
            name: "main/cpu/A".into(),
            width: 4,
            transitions: vec![Transition {
                time: 10,
                value: Signal::from_bits(vec![
                    Logic::Low,
                    Logic::High,
                    Logic::HighZ,
                    Logic::Unknown,
                ]),
            }],
        }];
        let source = export_vcd(&traces, 20).unwrap();
        assert!(source.contains("$timescale 1ns"));
        assert!(source.contains("\\main/cpu/A"));
        assert!(source.contains("#10\nbxz10 n0"));
    }
}
