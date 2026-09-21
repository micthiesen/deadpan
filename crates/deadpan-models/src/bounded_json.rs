use std::io::{self, Write};

use serde::Serialize;

pub(crate) fn encode(value: &impl Serialize, maximum: u64) -> Result<Vec<u8>, serde_json::Error> {
    let mut output = BoundedWriter {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut output, value)?;
    Ok(output.bytes)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: u64,
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self.maximum.saturating_sub(self.bytes.len() as u64);
        if bytes.len() as u64 > remaining {
            return Err(io::Error::other("host provenance exceeds its byte budget"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_json_stops_at_budget_without_partial_chunk() {
        let value = "\0".repeat(1024);
        assert!(encode(&value, 16).is_err());
        let mut writer = BoundedWriter {
            bytes: Vec::new(),
            maximum: 3,
        };
        writer.write_all(b"ab").unwrap();
        assert!(writer.write_all(b"cd").is_err());
        assert_eq!(writer.bytes, b"ab");
        assert_eq!(encode(&true, 4).unwrap(), b"true");
        assert!(encode(&true, 3).is_err());
    }
}
