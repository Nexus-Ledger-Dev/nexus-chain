use std::fs::OpenOptions;
use std::io::Write;
use serde::{Serialize, Deserialize};
use chrono::Utc;

/// Simple ISO‑compliant logger that writes immutable JSON audit entries.
/// Each entry includes a timestamp, a monotonically increasing sequence number,
/// and a SHA‑256 hash chain linking to the previous entry.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IsoLogEntry {
    pub seq: u64,
    pub timestamp: String,
    pub component: String,
    pub message: String,
    pub prev_hash: String,
    pub hash: String,
}

pub struct IsoLogger {
    path: String,
    seq: u64,
    prev_hash: String,
}

impl IsoLogger {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            seq: 0,
            prev_hash: String::from("0"),
        }
    }

    fn compute_hash(entry: &IsoLogEntry) -> String {
        let json = serde_json::to_string(entry).unwrap();
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest;
        hasher.update(json.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn log(&mut self, component: &str, message: &str) {
        self.seq += 1;
        let entry = IsoLogEntry {
            seq: self.seq,
            timestamp: Utc::now().to_rfc3339(),
            component: component.to_string(),
            message: message.to_string(),
            prev_hash: self.prev_hash.clone(),
            hash: String::new(), // placeholder, will be replaced
        };
        let hash = Self::compute_hash(&entry);
        let mut entry = entry;
        entry.hash = hash.clone();
        // Append to file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .expect("Unable to open iso log file");
        let line = serde_json::to_string(&entry).unwrap();
        writeln!(file, "{}", line).expect("Failed to write log");
        self.prev_hash = hash;
    }
}
