//! A single compiled assembly, never a general ZIP, pickle or recipe loader.

use crate::{AlignError, Control};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::io::Read;

#[path = "patches.rs"]
mod patches;

pub const CHECKPOINT_BYTES: usize = 57_562_900;
pub const CHECKPOINT_SHA256: &str =
    "4070c3b89fb27129e2999f550ddb55c747ec77da843fa8a520989b8c5d09723f";
pub const GRAPH_BYTES: usize = 19_219_161;
pub const GRAPH_SHA256: &str = "fe1e1890f968e7c19c0d4dde3ac7e15dfca15fd3bf2f9d5c41b2a5d1f120c57d";
const TEMPLATE_SHA256: &str = "298a8c669ee3fb8e0d50294eaa9317f535e2ba0ee6afde97663936d1ea58e770";
const GZIP_SHA256: &str = "31e5325dba8a7c25827997e0ce6df6887d7b63656069a6469cb29133ad79ac78";
const TEMPLATE: &[u8] = include_bytes!("../assets/model-template.onnx.gz");

struct Patch {
    name: &'static str,
    source: usize,
    destination: usize,
    length: usize,
    shape: &'static [usize],
    sha256: &'static str,
}

pub(crate) fn verify_hash(bytes: &[u8], expected: &str) -> bool {
    // Hex input is compiled, and no data-dependent filesystem reads occur.
    let actual = Sha256::digest(bytes);
    expected.len() == 64
        && actual.iter().enumerate().all(|(i, byte)| {
            let digits = b"0123456789abcdef";
            expected.as_bytes()[2 * i] == digits[usize::from(byte >> 4)]
                && expected.as_bytes()[2 * i + 1] == digits[usize::from(byte & 15)]
        })
}

/// Assemble the pinned graph from one already bounded, owned checkpoint read.
/// No archive metadata or executable serialization is interpreted.
pub fn assemble(checkpoint: &[u8], control: &Control) -> Result<Vec<u8>, AlignError> {
    control.check()?;
    if checkpoint.len() != CHECKPOINT_BYTES || !verify_hash(checkpoint, CHECKPOINT_SHA256) {
        return Err(AlignError::Integrity("checkpoint size or SHA256"));
    }
    control.check()?;
    if !verify_hash(TEMPLATE, GZIP_SHA256) {
        return Err(AlignError::Integrity("compiled compressed template"));
    }
    let mut graph = Vec::with_capacity(GRAPH_BYTES);
    GzDecoder::new(TEMPLATE)
        .take((GRAPH_BYTES + 1) as u64)
        .read_to_end(&mut graph)
        .map_err(|_| AlignError::Integrity("compiled template compression"))?;
    if graph.len() != GRAPH_BYTES || !verify_hash(&graph, TEMPLATE_SHA256) {
        return Err(AlignError::Integrity("compiled template size or SHA256"));
    }
    apply_patches(checkpoint, &mut graph, control)?;
    control.check()?;
    if !verify_hash(&graph, GRAPH_SHA256) {
        return Err(AlignError::Integrity("assembled graph SHA256"));
    }
    control.check()?;
    Ok(graph)
}

fn apply_patches(checkpoint: &[u8], graph: &mut [u8], control: &Control) -> Result<(), AlignError> {
    let mut previous_end = 0;
    for (index, patch) in patches::PATCHES.iter().enumerate() {
        control.check()?;
        let elements = patch
            .shape
            .iter()
            .try_fold(1_usize, |n, d| n.checked_mul(*d));
        if elements.and_then(|n| n.checked_mul(4)) != Some(patch.length)
            || patch.destination < previous_end
            || patches::PATCHES[..index]
                .iter()
                .any(|p| p.name == patch.name)
        {
            return Err(AlignError::Integrity("compiled tensor shape or overlap"));
        }
        let source_end = patch
            .source
            .checked_add(patch.length)
            .ok_or(AlignError::Integrity("compiled source range overflow"))?;
        let destination_end = patch
            .destination
            .checked_add(patch.length)
            .ok_or(AlignError::Integrity("compiled destination range overflow"))?;
        let source = checkpoint
            .get(patch.source..source_end)
            .ok_or(AlignError::Integrity("compiled source range"))?;
        let destination = graph
            .get_mut(patch.destination..destination_end)
            .ok_or(AlignError::Integrity("compiled destination range"))?;
        if destination.iter().any(|v| *v != 0) || !verify_hash(source, patch.sha256) {
            return Err(AlignError::Integrity(
                "tensor source hash or nonzero template range",
            ));
        }
        destination.copy_from_slice(source);
        previous_end = destination_end;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, atomic::AtomicBool},
        time::{Duration, Instant},
    };
    fn control() -> Control {
        Control::new(
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_secs(10),
        )
    }
    #[test]
    fn rejects_size_and_corrupted_checkpoint_without_interpreting_it() {
        assert!(matches!(
            assemble(b"PK\x03\x04", &control()),
            Err(AlignError::Integrity(_))
        ));
        assert!(matches!(
            assemble(&vec![0; CHECKPOINT_BYTES], &control()),
            Err(AlignError::Integrity(_))
        ));
    }
    #[test]
    fn compiled_template_and_tensor_ranges_are_sound() {
        assert!(verify_hash(TEMPLATE, GZIP_SHA256));
        let mut graph = Vec::new();
        GzDecoder::new(TEMPLATE).read_to_end(&mut graph).unwrap();
        assert_eq!(graph.len(), GRAPH_BYTES);
        assert!(verify_hash(&graph, TEMPLATE_SHA256));
        let mut end = 0;
        for patch in patches::PATCHES {
            assert!(patch.destination >= end);
            end = patch.destination + patch.length;
            assert!(end <= graph.len());
            assert!(patch.source + patch.length <= CHECKPOINT_BYTES);
            assert_eq!(patch.shape.iter().product::<usize>() * 4, patch.length);
            assert!(graph[patch.destination..end].iter().all(|v| *v == 0));
        }
    }
}
