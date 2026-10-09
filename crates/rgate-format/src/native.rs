use anyhow::{Context, Result, bail};
use rgate_core::Circuit;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    format: String,
    version: u32,
    circuit: Circuit,
}

pub fn encode(circuit: &Circuit) -> Result<String> {
    circuit.validate()?;
    let document = Document {
        format: "rgate".into(),
        version: FORMAT_VERSION,
        circuit: circuit.clone(),
    };
    Ok(serde_json::to_string_pretty(&document)? + "\n")
}

pub fn decode(source: &str) -> Result<Circuit> {
    let document: Document = serde_json::from_str(source).context("invalid RGate document")?;
    if document.format != "rgate" || document.version != FORMAT_VERSION {
        bail!(
            "unsupported RGate format/version: {} {}",
            document.format,
            document.version
        );
    }
    document
        .circuit
        .validate()
        .context("invalid circuit in RGate document")?;
    Ok(document.circuit)
}

/// Write next to the destination and rename only after the complete file is durable.
pub fn save_atomic(path: &Path, content: &str) -> Result<()> {
    let name = path
        .file_name()
        .context("save path has no filename")?
        .to_string_lossy();
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temporary =
        path.with_file_name(format!(".{name}.{}.{nonce}.rgate.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("could not create {}", temporary.display()))?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
            .with_context(|| format!("could not save {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() && temporary.exists() {
        fs::remove_file(&temporary).context("could not clean up failed save")?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgate_core::demo;

    #[test]
    fn native_round_trip_preserves_everything() {
        let circuit = demo::full_adder();
        assert_eq!(decode(&encode(&circuit).unwrap()).unwrap(), circuit);
    }

    #[test]
    fn future_versions_are_not_silently_accepted() {
        let source = encode(&Circuit::default())
            .unwrap()
            .replace("\"version\": 1", "\"version\": 99");
        assert!(
            decode(&source)
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );
    }

    #[test]
    fn invalid_references_are_rejected() {
        let mut circuit = demo::full_adder();
        circuit.modules[0].nets.clear();
        assert!(encode(&circuit).is_err());
    }

    #[test]
    fn atomic_save_replaces_previous_file() {
        let path =
            std::env::temp_dir().join(format!("rgate-save-test-{}.rgate", std::process::id()));
        save_atomic(&path, "old").unwrap();
        save_atomic(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        fs::remove_file(path).unwrap();
    }
}
