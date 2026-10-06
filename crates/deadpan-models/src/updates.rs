//! Signed update manifests for downloader helpers and model packs.
//!
//! Specification §15.2 and §27.3 require app-verified signed manifests for
//! helper and pack updates. Deadpan is a personal application without a
//! Developer ID (§27.1), so the trust root is a project-owned Ed25519 key: its
//! public half is compiled into the build from
//! [`models/update-keys.json`](../../../models/update-keys.json) and its private
//! half stays outside the repository, named at release time by path
//! (`deadpan-cli update-signing sign --key <file>` or `DEADPAN_UPDATE_SIGNING_KEY`).
//!
//! A signed manifest is a small JSON envelope whose `payload` is the exact
//! manifest text. The signature covers a domain-separated message
//! (`deadpan-signed-update-v1`, the update kind and the payload bytes), so a
//! downloader manifest can never be replayed as a pack manifest or vice versa.
//! Verification happens before the payload is parsed. Ed25519 comes from
//! `ring`, which the HTTPS transport already links (Apache-2.0 AND ISC).

use std::cmp::Ordering;

use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Envelope schema.
pub const SIGNED_SCHEMA: u32 = 1;
/// Upper bound for an envelope read from a file or the network.
pub const MAX_SIGNED_BYTES: u64 = 256 * 1024;
const DOMAIN: &[u8] = b"deadpan-signed-update-v1\0";

/// The workspace version this build reports to `min_app_version` checks.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("signed update manifest is malformed: {0}")]
    Malformed(String),
    #[error("signed update manifest is for {found}, not {expected}")]
    WrongKind { expected: String, found: String },
    /// No compiled key has this identifier: the key was rotated out of this
    /// build, or the manifest was signed by someone else.
    #[error("signed update manifest names unknown signing key {0}")]
    UnknownKey(String),
    #[error("signed update manifest failed signature verification")]
    BadSignature,
    #[error("signing key is unusable: {0}")]
    Key(String),
}

/// What a signed manifest updates; part of the signed message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateKind {
    Downloader,
    ModelPack,
}

impl UpdateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Downloader => "downloader",
            Self::ModelPack => "model-pack",
        }
    }
}

/// One trusted public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedKey {
    pub id: String,
    pub public_key: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyFile {
    schema: u32,
    keys: Vec<KeyEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyEntry {
    id: String,
    ed25519: String,
}

/// The keys compiled into this build.
pub fn trusted_keys() -> Vec<TrustedKey> {
    parse_keys(include_str!("../../../models/update-keys.json"))
        .expect("compiled update keys parse")
}

fn parse_keys(text: &str) -> Result<Vec<TrustedKey>, UpdateError> {
    let file: KeyFile =
        serde_json::from_str(text).map_err(|error| UpdateError::Malformed(error.to_string()))?;
    if file.schema != 1 {
        return Err(UpdateError::Malformed("unsupported key file schema".into()));
    }
    file.keys
        .into_iter()
        .map(|entry| {
            let bytes = decode_hex(&entry.ed25519)
                .filter(|bytes| bytes.len() == 32)
                .ok_or_else(|| UpdateError::Malformed("public key must be 32 hex bytes".into()))?;
            if !key_id(&entry.id) {
                return Err(UpdateError::Malformed(
                    "key id must be a safe identifier".into(),
                ));
            }
            Ok(TrustedKey {
                id: entry.id,
                public_key: bytes.try_into().expect("checked length"),
            })
        })
        .collect()
}

fn key_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// The signed envelope exactly as stored and transported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedManifest {
    pub schema: u32,
    pub kind: UpdateKind,
    pub key_id: String,
    /// The manifest JSON text; the signature covers these exact bytes.
    pub payload: String,
    /// Lowercase hex Ed25519 signature.
    pub signature: String,
}

fn message(kind: UpdateKind, payload: &str) -> Vec<u8> {
    let mut message = Vec::with_capacity(DOMAIN.len() + 16 + payload.len());
    message.extend_from_slice(DOMAIN);
    message.extend_from_slice(kind.as_str().as_bytes());
    message.push(0);
    message.extend_from_slice(payload.as_bytes());
    message
}

impl SignedManifest {
    /// Parse a bounded envelope. Nothing in the payload is trusted yet.
    pub fn parse(bytes: &[u8]) -> Result<Self, UpdateError> {
        if bytes.len() as u64 > MAX_SIGNED_BYTES {
            return Err(UpdateError::Malformed("larger than 256 KiB".into()));
        }
        let envelope: Self = serde_json::from_slice(bytes)
            .map_err(|error| UpdateError::Malformed(error.to_string()))?;
        if envelope.schema != SIGNED_SCHEMA {
            return Err(UpdateError::Malformed(format!(
                "unsupported envelope schema {}",
                envelope.schema
            )));
        }
        Ok(envelope)
    }

    /// Verify against the compiled keys and return the trusted payload text.
    pub fn verify(&self, kind: UpdateKind) -> Result<&str, UpdateError> {
        self.verify_with(kind, &trusted_keys())
    }

    /// Verify against explicit keys (tests and key rotation checks).
    pub fn verify_with(&self, kind: UpdateKind, keys: &[TrustedKey]) -> Result<&str, UpdateError> {
        if self.kind != kind {
            return Err(UpdateError::WrongKind {
                expected: kind.as_str().into(),
                found: self.kind.as_str().into(),
            });
        }
        let key = keys
            .iter()
            .find(|key| key.id == self.key_id)
            .ok_or_else(|| UpdateError::UnknownKey(self.key_id.clone()))?;
        let signature = decode_hex(&self.signature)
            .filter(|bytes| bytes.len() == 64)
            .ok_or(UpdateError::BadSignature)?;
        UnparsedPublicKey::new(&ED25519, key.public_key)
            .verify(&message(self.kind, &self.payload), &signature)
            .map_err(|_| UpdateError::BadSignature)?;
        Ok(&self.payload)
    }

    /// Sign `payload` with a PKCS#8 v2 Ed25519 private key.
    pub fn sign(
        kind: UpdateKind,
        payload: String,
        key_id: &str,
        pkcs8: &[u8],
    ) -> Result<Self, UpdateError> {
        if !self::key_id(key_id) {
            return Err(UpdateError::Key("key id must be a safe identifier".into()));
        }
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8)
            .map_err(|error| UpdateError::Key(error.to_string()))?;
        let signature = pair.sign(&message(kind, &payload));
        Ok(Self {
            schema: SIGNED_SCHEMA,
            kind,
            key_id: key_id.into(),
            payload,
            signature: encode_hex(signature.as_ref()),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).expect("envelope serializes");
        bytes.push(b'\n');
        bytes
    }
}

/// A new PKCS#8 v2 Ed25519 private key and its public key.
pub fn generate_key() -> Result<(Vec<u8>, [u8; 32]), UpdateError> {
    let random = ring::rand::SystemRandom::new();
    let document = Ed25519KeyPair::generate_pkcs8(&random)
        .map_err(|error| UpdateError::Key(error.to_string()))?;
    let public = public_key(document.as_ref())?;
    Ok((document.as_ref().to_vec(), public))
}

/// The public key of a PKCS#8 v2 Ed25519 private key.
pub fn public_key(pkcs8: &[u8]) -> Result<[u8; 32], UpdateError> {
    let pair =
        Ed25519KeyPair::from_pkcs8(pkcs8).map_err(|error| UpdateError::Key(error.to_string()))?;
    pair.public_key()
        .as_ref()
        .try_into()
        .map_err(|_| UpdateError::Key("unexpected public key length".into()))
}

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2)
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

/// Compare dotted numeric versions (`2026.08.19`, `2.9.7`, `1`). Missing
/// trailing components count as zero. `None` for anything else, which callers
/// treat as incomparable and refuse.
pub fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    let parse = |value: &str| -> Option<Vec<u64>> {
        if value.is_empty() || value.len() > 64 {
            return None;
        }
        value
            .split('.')
            .map(|part| {
                (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
                    .then(|| part.parse::<u64>().ok())
                    .flatten()
            })
            .collect()
    };
    let (left, right) = (parse(left)?, parse(right)?);
    let length = left.len().max(right.len());
    let component = |parts: &[u64], index: usize| parts.get(index).copied().unwrap_or(0);
    Some(
        (0..length)
            .map(|index| component(&left, index).cmp(&component(&right, index)))
            .find(|ordering| ordering.is_ne())
            .unwrap_or(Ordering::Equal),
    )
}

/// Whether this build satisfies a manifest's `min_app_version`.
pub fn app_satisfies(min_app_version: &str) -> Option<bool> {
    compare_versions(APP_VERSION, min_app_version).map(Ordering::is_ge)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key(id: &str) -> (Vec<u8>, TrustedKey) {
        let (pkcs8, public_key) = generate_key().unwrap();
        (
            pkcs8,
            TrustedKey {
                id: id.into(),
                public_key,
            },
        )
    }

    #[test]
    fn signatures_bind_kind_key_and_payload() {
        let (pkcs8, key) = test_key("test-1");
        let signed =
            SignedManifest::sign(UpdateKind::Downloader, "{\"a\":1}".into(), "test-1", &pkcs8)
                .unwrap();
        let parsed = SignedManifest::parse(&signed.to_bytes()).unwrap();
        assert_eq!(
            parsed.verify_with(UpdateKind::Downloader, std::slice::from_ref(&key)),
            Ok("{\"a\":1}")
        );
        // Another kind is never accepted, even with a relabelled envelope.
        assert!(matches!(
            parsed.verify_with(UpdateKind::ModelPack, std::slice::from_ref(&key)),
            Err(UpdateError::WrongKind { .. })
        ));
        let relabelled = SignedManifest {
            kind: UpdateKind::ModelPack,
            ..parsed.clone()
        };
        assert_eq!(
            relabelled.verify_with(UpdateKind::ModelPack, std::slice::from_ref(&key)),
            Err(UpdateError::BadSignature)
        );
        // A changed payload byte fails.
        let tampered = SignedManifest {
            payload: "{\"a\":2}".into(),
            ..parsed.clone()
        };
        assert_eq!(
            tampered.verify_with(UpdateKind::Downloader, std::slice::from_ref(&key)),
            Err(UpdateError::BadSignature)
        );
        // A different key under the same id fails; an unknown id is distinct.
        let (_, other) = test_key("test-1");
        assert_eq!(
            parsed.verify_with(UpdateKind::Downloader, &[other]),
            Err(UpdateError::BadSignature)
        );
        assert_eq!(
            parsed.verify_with(UpdateKind::Downloader, &[]),
            Err(UpdateError::UnknownKey("test-1".into()))
        );
        // Malformed signatures and envelopes.
        let short = SignedManifest {
            signature: "00".into(),
            ..parsed.clone()
        };
        assert_eq!(
            short.verify_with(UpdateKind::Downloader, &[key]),
            Err(UpdateError::BadSignature)
        );
        assert!(SignedManifest::parse(b"{}").is_err());
        assert!(SignedManifest::parse(&vec![b' '; 300 * 1024]).is_err());
        let mut extra: serde_json::Value = serde_json::from_slice(&signed.to_bytes()).unwrap();
        extra["unexpected"] = 1.into();
        assert!(SignedManifest::parse(&serde_json::to_vec(&extra).unwrap()).is_err());
    }

    #[test]
    fn compiled_keys_parse() {
        let keys = trusted_keys();
        assert!(!keys.is_empty());
        assert!(parse_keys("{\"schema\":1,\"keys\":[{\"id\":\"a\",\"ed25519\":\"00\"}]}").is_err());
        assert!(parse_keys("{\"schema\":2,\"keys\":[]}").is_err());
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(
            compare_versions("2026.08.19", "2026.10.1"),
            Some(Ordering::Less)
        );
        assert_eq!(compare_versions("2.10.0", "2.9.7"), Some(Ordering::Greater));
        assert_eq!(compare_versions("2", "2.0.0"), Some(Ordering::Equal));
        assert_eq!(
            compare_versions("2026.08.19.1", "2026.08.19"),
            Some(Ordering::Greater)
        );
        assert_eq!(compare_versions("1.0-beta", "1.0"), None);
        assert_eq!(compare_versions("", "1"), None);
        assert_eq!(compare_versions("1..2", "1"), None);
        assert_eq!(app_satisfies("0.0.1"), Some(true));
        assert_eq!(app_satisfies("999.0"), Some(false));
        assert_eq!(app_satisfies("latest"), None);
    }
}
