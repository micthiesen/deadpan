//! Release-time signing of update manifests and loading of signed manifests.
//!
//! `update-signing keygen --out <file>` creates the project's Ed25519 key
//! (PKCS#8 v2, owner-only, never inside the repository) and prints the public
//! key for `models/update-keys.json`. `update-signing sign` signs one
//! downloader or model-pack manifest with the key named by `--key` or
//! `DEADPAN_UPDATE_SIGNING_KEY`. Signing refuses a key this build does not
//! trust, so a manifest is never published that the app would reject.
//! See docs/UPDATES.md.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use deadpan_models::packs::{HttpsTransport, Transport};
use deadpan_models::updates::{
    MAX_SIGNED_BYTES, SignedManifest, UpdateError, UpdateKind, encode_hex, generate_key,
    public_key, trusted_keys,
};

use crate::CliError;
use crate::youtube::ImportError;

/// Environment variable naming the private key file at release time.
pub const KEY_ENVIRONMENT: &str = "DEADPAN_UPDATE_SIGNING_KEY";
const MAX_PAYLOAD_BYTES: u64 = 192 * 1024;
const MAX_DESCRIBED_BYTES: u64 = 512 * 1024 * 1024;

const USAGE: &str = "usage: update-signing keygen --out <new key file> | update-signing sign --kind downloader|model-pack [--key <key file>] <manifest.json> <signed.json> | update-signing verify --kind downloader|model-pack <signed.json> | update-signing describe <helper executable>";

fn usage() -> CliError {
    CliError::Usage(USAGE.into())
}

pub(crate) fn update_error(code: &'static str, error: impl std::fmt::Display) -> CliError {
    CliError::Import(ImportError::new(code, error.to_string()))
}

/// Map a verification failure to a stable code.
pub fn verification_error(error: UpdateError) -> CliError {
    let code = match error {
        UpdateError::UnknownKey(_) => "UpdateUntrusted",
        UpdateError::BadSignature | UpdateError::WrongKind { .. } => "UpdateSignatureInvalid",
        UpdateError::Malformed(_) | UpdateError::Key(_) => "UpdateManifestInvalid",
    };
    update_error(code, error)
}

/// Read a signed manifest from an HTTPS URL or a local file, bounded.
pub fn read_signed(source: &str, user_agent: &str) -> Result<Vec<u8>, CliError> {
    let mut bytes = Vec::new();
    if source.starts_with("https://") {
        let download = HttpsTransport::with_user_agent(user_agent)
            .fetch(source, 0)
            .map_err(|error| update_error("UpdateFetchFailed", error))?;
        if download.offset != 0 {
            return Err(update_error(
                "UpdateFetchFailed",
                "server answered with a partial response",
            ));
        }
        download
            .body
            .take(MAX_SIGNED_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| update_error("UpdateFetchFailed", error))?;
    } else if source.contains("://") {
        return Err(update_error(
            "UpdateManifestInvalid",
            "signed manifests come from a local file or an https:// URL",
        ));
    } else {
        let metadata = fs::metadata(source)?;
        if !metadata.is_file() {
            return Err(update_error(
                "UpdateManifestInvalid",
                format!("{source} is not a file"),
            ));
        }
        File::open(source)?
            .take(MAX_SIGNED_BYTES + 1)
            .read_to_end(&mut bytes)?;
    }
    if bytes.len() as u64 > MAX_SIGNED_BYTES {
        return Err(update_error(
            "UpdateManifestInvalid",
            "signed manifest is larger than 256 KiB",
        ));
    }
    Ok(bytes)
}

fn kind(value: &str) -> Result<UpdateKind, CliError> {
    match value {
        "downloader" => Ok(UpdateKind::Downloader),
        "model-pack" => Ok(UpdateKind::ModelPack),
        _ => Err(usage()),
    }
}

fn read_key(path: &Path) -> Result<Vec<u8>, CliError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(update_error(
            "UpdateKeyInvalid",
            format!(
                "{} is readable by other users; chmod 600 it",
                path.display()
            ),
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(4096).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The compiled key id matching this private key.
fn trusted_id(pkcs8: &[u8]) -> Result<String, CliError> {
    let public = public_key(pkcs8).map_err(|error| update_error("UpdateKeyInvalid", error))?;
    trusted_keys()
        .into_iter()
        .find(|key| key.public_key == public)
        .map(|key| key.id)
        .ok_or_else(|| {
            update_error(
                "UpdateUntrusted",
                format!(
                    "this build does not trust public key {}; add it to models/update-keys.json and rebuild first",
                    encode_hex(&public)
                ),
            )
        })
}

/// Sign `payload` with the key file, as the compiled key it matches.
pub fn sign(kind: UpdateKind, payload: String, key: &Path) -> Result<SignedManifest, CliError> {
    let pkcs8 = read_key(key)?;
    let id = trusted_id(&pkcs8)?;
    let signed = SignedManifest::sign(kind, payload, &id, &pkcs8)
        .map_err(|error| update_error("UpdateKeyInvalid", error))?;
    // Never publish something this build would reject.
    signed.verify(kind).map_err(verification_error)?;
    Ok(signed)
}

fn write_new(path: &Path, bytes: &[u8], mode: u32) -> Result<(), CliError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// `update-signing keygen|sign|verify`
pub(crate) fn run(arguments: &[&str]) -> Result<(), CliError> {
    match arguments {
        ["keygen", "--out", out] => {
            let out = PathBuf::from(out);
            let (pkcs8, public) =
                generate_key().map_err(|error| update_error("UpdateKeyInvalid", error))?;
            if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
                fs::create_dir_all(parent)?;
            }
            write_new(&out, &pkcs8, 0o600)?;
            crate::write_json(&serde_json::json!({
                "protocol": 1,
                "private_key": out,
                "ed25519": encode_hex(&public),
            }))
        }
        ["sign", "--kind", kind_name, rest @ ..] => {
            let kind = kind(kind_name)?;
            let (key, manifest, output) = match rest {
                ["--key", key, manifest, output] => (PathBuf::from(key), manifest, output),
                [manifest, output] => (
                    std::env::var_os(KEY_ENVIRONMENT)
                        .map(PathBuf::from)
                        .ok_or_else(|| {
                            CliError::Usage(format!("pass --key or set {KEY_ENVIRONMENT}"))
                        })?,
                    manifest,
                    output,
                ),
                _ => return Err(usage()),
            };
            let mut payload = String::new();
            File::open(manifest)?
                .take(MAX_PAYLOAD_BYTES + 1)
                .read_to_string(&mut payload)?;
            if payload.len() as u64 > MAX_PAYLOAD_BYTES {
                return Err(update_error(
                    "UpdateManifestInvalid",
                    "manifest is larger than 192 KiB",
                ));
            }
            // Refuse a payload the app cannot parse before signing it.
            match kind {
                UpdateKind::Downloader => {
                    crate::youtube::updates::DownloaderManifest::parse(&payload)?;
                }
                UpdateKind::ModelPack => {
                    crate::models::parse_pack_update(&payload)?;
                }
            }
            let signed = sign(kind, payload, &key)?;
            write_new(Path::new(output), &signed.to_bytes(), 0o644)?;
            crate::write_json(&serde_json::json!({
                "protocol": 1,
                "signed": output,
                "kind": kind,
                "key_id": signed.key_id,
            }))
        }
        ["describe", file] => {
            // Release authoring: the exact hash, size and Mach-O content pin
            // of a helper file for a downloader manifest.
            let mut bytes = Vec::new();
            File::open(file)?
                .take(MAX_DESCRIBED_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_DESCRIBED_BYTES {
                return Err(CliError::Usage(format!("{file} is larger than 512 MiB")));
            }
            use sha2::Digest;
            crate::write_json(&serde_json::json!({
                "protocol": 1,
                "file": file,
                "bytes": bytes.len(),
                "sha256": encode_hex(&sha2::Sha256::digest(&bytes)),
                "content_sha256": crate::youtube::macho_content::content_sha256(&bytes).ok(),
            }))
        }
        ["verify", "--kind", kind_name, signed] => {
            let kind = kind(kind_name)?;
            let bytes = read_signed(signed, crate::youtube::helpers::USER_AGENT)?;
            let envelope = SignedManifest::parse(&bytes).map_err(verification_error)?;
            envelope.verify(kind).map_err(verification_error)?;
            crate::write_json(&serde_json::json!({
                "protocol": 1,
                "verified": true,
                "kind": kind,
                "key_id": envelope.key_id,
            }))
        }
        _ => Err(usage()),
    }
}
