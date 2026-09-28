//! Byte-bounded standalone JSON. No general ExactRatio wire changes are made.

use std::io::{self, Write};

use crate::{DocumentError, DocumentErrorCode};

use super::{AudioTreatments, GainError, MAX_AUDIO_TREATMENTS_JSON_BYTES};

impl AudioTreatments {
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        if json.len() > MAX_AUDIO_TREATMENTS_JSON_BYTES {
            return Err(limit());
        }
        serde_json::from_str(json).map_err(DocumentError::json)
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.validate().map_err(|error| {
            DocumentError::new(
                if error == GainError::Limit {
                    DocumentErrorCode::LimitExceeded
                } else {
                    DocumentErrorCode::InvalidJson
                },
                error.to_string(),
            )
        })?;
        let mut output = BoundedJson::default();
        serde_json::to_writer(&mut output, self).map_err(|error| {
            if output.exceeded {
                limit()
            } else {
                DocumentError::json(error)
            }
        })?;
        String::from_utf8(output.bytes)
            .map_err(|error| DocumentError::new(DocumentErrorCode::InvalidJson, error.to_string()))
    }
}

fn limit() -> DocumentError {
    DocumentError::new(
        DocumentErrorCode::LimitExceeded,
        "gain JSON exceeds 512 KiB byte limit",
    )
}

#[derive(Default)]
struct BoundedJson {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > MAX_AUDIO_TREATMENTS_JSON_BYTES)
        {
            self.exceeded = true;
            return Err(io::Error::other("gain JSON byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
