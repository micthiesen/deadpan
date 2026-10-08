//! Raw conditioning identities from retained evidence, without opening media.
//!
//! Source selections resolve through their measured presentation index. A
//! changed affine span or selection does not change the input when it still
//! selects the same picture. Generated inputs use the sampled master, because
//! that is the movie the conditioning worker actually decodes.

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::{cell::RefCell, collections::BTreeMap};

#[cfg(any(target_os = "macos", target_os = "linux"))]
use deadpan_core::AssetRecord;
use deadpan_core::{GeneratedObjectRef, ProjectDocument, SourceFrameId, SourceQualificationId};
use deadpan_plan::Picture;
use rusqlite::Connection;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::{ProjectStore, StoreError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GenerationPictureIdentity {
    Original {
        qualification: SourceQualificationId,
        frame: SourceFrameId,
    },
    Generated {
        sampled_object: GeneratedObjectRef,
        frame: SourceFrameId,
        /// Reduced positive ratio used by the conditioning decoder's centered
        /// fill_canvas_aspect crop. None preserves the complete decoded raster.
        /// Raster dimensions are unavailable here: different ratios which round
        /// to the same pixel crop may conservatively compare unequal. Never
        /// infer those dimensions or treat None as an assumed native aspect.
        content_aspect: Option<[u32; 2]>,
    },
    AuthoredBlack,
}

/// Transition-local access to measured identities. Implementations must never
/// substitute an inferred frame rate or decode media on the writer's behalf.
pub trait GenerationPictures {
    fn identity(
        &self,
        document: &ProjectDocument,
        picture: &Picture,
    ) -> Result<GenerationPictureIdentity, StoreError>;
}

/// Receipt indexes and asset contracts are loaded once per qualification for
/// this observation batch, even if many aliases name the same Original. Failed
/// loads also consume their reserved work and are remembered for the batch.
/// The connection is borrowed so no revision or admission can change mid-batch.
pub struct QualifiedGenerationPictures<'a> {
    connection: &'a Connection,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    receipts: RefCell<ReceiptCache>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[derive(Default)]
struct ReceiptCache {
    entries: BTreeMap<SourceQualificationId, Result<CachedReceipt, String>>,
    bytes: usize,
    frames: usize,
    exhausted: Option<&'static str>,
    #[cfg(test)]
    contract_builds: usize,
}

// One legal maximum-size receipt fits, while many small-video/long-audio
// receipts cannot retain an unbounded collection of audio observations.
// These are transition-wide work limits, not per-asset limits. Bytes are the
// stored canonical snapshot envelope (video, audio and interpretation), charged
// before loading. Parsed metadata and its derived indexes also occupy memory.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const MAX_RECEIPT_BYTES: usize =
    deadpan_media::source_qualification::MAX_SOURCE_QUALIFICATION_JSON_BYTES;
#[cfg(any(target_os = "macos", target_os = "linux"))]
const MAX_RECEIPTS: usize = 256;

#[cfg(any(target_os = "macos", target_os = "linux"))]
struct CachedReceipt {
    receipt: crate::source_registration::SourceQualificationReceipt,
    contract: AssetRecord,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl CachedReceipt {
    fn matches(&self, record: &AssetRecord) -> bool {
        // The caller's label is editorial; every other asset field belongs to
        // the qualified contract. Destructure exhaustively so new fields cannot
        // silently escape this comparison.
        let AssetRecord {
            label: _,
            content_hash,
            video,
            audio,
            still_image,
            frame_count,
            source_qualification,
        } = record;
        &self.contract.content_hash == content_hash
            && &self.contract.video == video
            && &self.contract.audio == audio
            && &self.contract.still_image == still_image
            && &self.contract.frame_count == frame_count
            && &self.contract.source_qualification == source_qualification
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl ReceiptCache {
    fn get(
        &mut self,
        connection: &Connection,
        id: &SourceQualificationId,
    ) -> Result<&CachedReceipt, StoreError> {
        if !self.entries.contains_key(id) {
            if let Some(reason) = self.exhausted {
                return Err(invalid(reason));
            }
            if self.entries.len() == MAX_RECEIPTS {
                return self.exhaust("conditioning receipt count exceeds the aggregate bound");
            }
            if self.bytes == MAX_RECEIPT_BYTES {
                return self.exhaust("conditioning receipts exceed the aggregate byte bound");
            }
            if self.frames == deadpan_core::MAX_SOURCE_INDEX_FRAMES {
                return self.exhaust("conditioning indexes exceed the aggregate frame bound");
            }
            let result = self.load(connection, id).map_err(|error| error.to_string());
            self.entries.insert(id.clone(), result);
        }
        self.entries[id].as_ref().map_err(|reason| invalid(reason))
    }

    fn exhaust<T>(&mut self, reason: &'static str) -> Result<T, StoreError> {
        self.exhausted = Some(reason);
        Err(invalid(reason))
    }

    fn load(
        &mut self,
        connection: &Connection,
        id: &SourceQualificationId,
    ) -> Result<CachedReceipt, StoreError> {
        // SQLite can inspect a BLOB length without returning its bytes. This
        // precedes read_receipt's allocation, parsing, canonicalization and
        // hashing. Missing and malformed rows are cached by get as failures.
        let bytes: Option<Option<i64>> = connection
            .query_row(
                "SELECT CASE WHEN typeof(snapshot)='blob' THEN length(snapshot) END
             FROM source_qualifications WHERE id=?1",
                [id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        let bytes = bytes
            .ok_or_else(|| invalid("conditioning source qualification is missing"))?
            .and_then(|length| usize::try_from(length).ok())
            .filter(|length| *length != 0)
            .ok_or_else(|| invalid("conditioning qualification snapshot is not a nonempty blob"))?;
        let Some(total) = self
            .bytes
            .checked_add(bytes)
            .filter(|total| *total <= MAX_RECEIPT_BYTES)
        else {
            return self.exhaust("conditioning receipts exceed the aggregate byte bound");
        };
        // Failed decodes are work already spent; do not return their budget.
        self.bytes = total;
        let receipt = crate::source_registration::read_receipt(connection, id)?
            .ok_or_else(|| invalid("conditioning source qualification is missing"))?;
        let count = receipt
            .snapshot()
            .video()
            .ok_or_else(|| invalid("conditioning source has no selected picture stream"))?
            .index()
            .index()
            .frames()
            .len();
        let Some(total) = self
            .frames
            .checked_add(count)
            .filter(|total| *total <= deadpan_core::MAX_SOURCE_INDEX_FRAMES)
        else {
            return self.exhaust("conditioning indexes exceed the aggregate frame bound");
        };
        self.frames = total;
        // derive_timing scans the complete audio index. Reconstruct this once,
        // never for each endpoint or each differently labelled asset alias.
        let contract = receipt.asset_record(String::new())?;
        #[cfg(test)]
        {
            self.contract_builds += 1;
        }
        Ok(CachedReceipt { receipt, contract })
    }
}

impl<'a> QualifiedGenerationPictures<'a> {
    pub(crate) fn new(connection: &'a Connection) -> Self {
        Self {
            connection,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            receipts: RefCell::new(ReceiptCache::default()),
        }
    }
}

impl ProjectStore {
    /// Also accepts prospective documents: their asset records identify the
    /// admitted receipts directly, without looking up an uncommitted revision.
    pub fn generation_pictures(&self) -> QualifiedGenerationPictures<'_> {
        QualifiedGenerationPictures::new(&self.connection)
    }
}

impl GenerationPictures for QualifiedGenerationPictures<'_> {
    fn identity(
        &self,
        document: &ProjectDocument,
        picture: &Picture,
    ) -> Result<GenerationPictureIdentity, StoreError> {
        match picture {
            Picture::Blank | Picture::Background => Ok(GenerationPictureIdentity::AuthoredBlack),
            Picture::Accepted {
                asset,
                generated: Some(artifact),
                frame,
                ..
            } => {
                if asset != &artifact.sampled_asset
                    || frame.0
                        >= u64::try_from(artifact.sampling.output_frame_count().frames())
                            .map_err(|_| invalid("generated frame extent is not representable"))?
                {
                    return Err(invalid(
                        "generated conditioning picture disagrees with its sampled master",
                    ));
                }
                Ok(GenerationPictureIdentity::Generated {
                    sampled_object: artifact.sampled_object.clone(),
                    frame: *frame,
                    content_aspect: normalized_aspect(artifact.content_aspect)?,
                })
            }
            Picture::Source { asset, .. } | Picture::Freeze { asset, .. } => {
                let record = document
                    .assets()
                    .get(asset)
                    .ok_or_else(|| invalid("conditioning asset is absent"))?;
                let qualification = record
                    .source_qualification
                    .as_ref()
                    .ok_or_else(|| invalid("conditioning source has no measured qualification"))?;
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                {
                    let mut receipts = self.receipts.borrow_mut();
                    let cached = receipts.get(self.connection, qualification)?;
                    if !cached.matches(record) {
                        return Err(invalid(
                            "conditioning asset disagrees with its source qualification",
                        ));
                    }
                    let index = cached
                        .receipt
                        .snapshot()
                        .video()
                        .ok_or_else(|| {
                            invalid("conditioning source has no selected picture stream")
                        })?
                        .index()
                        .index();
                    original_identity(qualification, index, picture)
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    let _ = (qualification, self.connection);
                    Err(invalid(
                        "measured generation pictures are unavailable on this platform",
                    ))
                }
            }
            Picture::Still { .. }
            | Picture::Accepted {
                generated: None, ..
            } => Err(invalid(
                "conditioning picture lacks retained qualified video evidence",
            )),
        }
    }
}

fn normalized_aspect(aspect: Option<[u32; 2]>) -> Result<Option<[u32; 2]>, StoreError> {
    let Some([width, height]) = aspect else {
        return Ok(None);
    };
    if width == 0 || height == 0 {
        return Err(invalid(
            "generated conditioning crop needs a positive content aspect",
        ));
    }
    let (mut a, mut b) = (width, height);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Ok(Some([width / a, height / a]))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn original_identity(
    qualification: &SourceQualificationId,
    index: &deadpan_core::SourceFrameIndex,
    picture: &Picture,
) -> Result<GenerationPictureIdentity, StoreError> {
    // Rebind the cheap picture descriptor to the receipt's canonical alias;
    // cloning a potentially large measured index for each authored alias would
    // make an otherwise small structural edit scale with source duration.
    let mut picture = picture.clone();
    match &mut picture {
        Picture::Source { asset, .. } | Picture::Freeze { asset, .. } => {
            *asset = index.asset().clone();
        }
        _ => return Err(invalid("expected an Original conditioning picture")),
    }
    let frame = picture
        .select_source_frame(index)
        .map_err(|error| invalid(&format!("conditioning frame cannot be resolved: {error}")))?;
    Ok(GenerationPictureIdentity::Original {
        qualification: qualification.clone(),
        frame: frame.identity,
    })
}

fn invalid(reason: &str) -> StoreError {
    StoreError::GenerationPlan(reason.into())
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests;
