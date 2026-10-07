//! Aggregated third-party notices for the pinned Deno helper.
//!
//! Deno publishes only its own MIT license. `tools/notices/deno_notices.py`
//! collects the license files of everything statically linked into the pinned
//! executable (rusty_v8, V8 and its third-party C/C++ sources, the Rust
//! standard library and Deno's crates.io closure) at the exact upstream
//! revisions into `packaging/notices/deno-<version>/`: a `manifest.json` and
//! each distinct text once as `texts/<sha256>.txt`. The bundle renders one
//! deterministic `deno/THIRD_PARTY_NOTICES.txt` from that set and copies the
//! manifest beside it; the audit re-renders it and compares bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::spdx;
use super::{Result, sha256_hex};

pub const SCHEMA: u64 = 1;
/// The aggregated notice, relative to `Contents/Resources/Notices`.
pub const BUNDLED_NOTICES: &str = "deno/THIRD_PARTY_NOTICES.txt";
/// The vendored manifest copied byte-for-byte, relative to the notices.
pub const BUNDLED_MANIFEST: &str = "deno/notices-manifest.json";
/// The bom-ref of the Deno helper component in the SBOM.
pub const SBOM_HELPER: &str = "helper:deno";

/// `packaging/notices/deno-<version>` under `notice_sets` (`packaging/notices`).
pub fn set_directory(notice_sets: &Path, version: &str) -> Result<PathBuf> {
    if version.is_empty()
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+')
        || version.starts_with('.')
    {
        return Err(format!("invalid Deno version {version:?}"));
    }
    Ok(notice_sets.join(format!("deno-{version}")))
}

/// One verified notice set.
pub struct DenoNotices {
    pub version: String,
    pub executable_sha256: String,
    manifest_bytes: Vec<u8>,
    manifest: Value,
    texts: BTreeMap<String, String>,
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("Deno notice manifest: missing {field}"))
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .ok_or_else(|| format!("Deno notice manifest: missing {field} list"))
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl DenoNotices {
    /// Read and verify a notice set: every text matches its hash, the text
    /// directory holds exactly the listed texts, every reference resolves and
    /// every crate without a license file is covered by a bundled standard
    /// text.
    pub fn load(root: &Path, identifiers: &spdx::Identifiers) -> Result<Self> {
        let manifest_path = root.join("manifest.json");
        let manifest_bytes =
            fs::read(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let manifest: Value = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        if manifest["schema"].as_u64() != Some(SCHEMA) {
            return Err(format!(
                "{}: unsupported schema {}",
                manifest_path.display(),
                manifest["schema"]
            ));
        }
        let deno = &manifest["deno"];
        let version = string(deno, "version")?.to_owned();
        let executable_sha256 = string(deno, "executable_sha256")?.to_owned();
        if !is_sha256(&executable_sha256) {
            return Err("Deno notice manifest: invalid executable SHA-256".into());
        }
        for field in ["tag", "commit", "release"] {
            string(deno, field)?;
        }
        for field in ["version", "tag", "commit"] {
            string(&manifest["rusty_v8"], field)?;
        }

        let listed: BTreeSet<String> = array(&manifest, "texts")?
            .iter()
            .map(|value| value.as_str().map(str::to_owned))
            .collect::<Option<_>>()
            .ok_or("Deno notice manifest: texts must be strings")?;
        let directory = root.join("texts");
        let mut present = BTreeSet::new();
        for entry in
            fs::read_dir(&directory).map_err(|e| format!("{}: {e}", directory.display()))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            present.insert(entry.file_name().to_string_lossy().into_owned());
        }
        let expected: BTreeSet<String> = listed.iter().map(|hash| format!("{hash}.txt")).collect();
        if present != expected {
            let extra: Vec<_> = present.difference(&expected).collect();
            let missing: Vec<_> = expected.difference(&present).collect();
            return Err(format!(
                "{}: text files differ from the manifest (unlisted {extra:?}, missing {missing:?})",
                directory.display()
            ));
        }
        let mut texts = BTreeMap::new();
        for hash in &listed {
            if !is_sha256(hash) {
                return Err(format!("Deno notice manifest: invalid text hash {hash}"));
            }
            let path = directory.join(format!("{hash}.txt"));
            let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            if sha256_hex(&bytes) != *hash {
                return Err(format!(
                    "{} does not match its recorded SHA-256",
                    path.display()
                ));
            }
            let text =
                String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8", path.display()))?;
            texts.insert(hash.clone(), text);
        }

        let notices = Self {
            version,
            executable_sha256,
            manifest_bytes,
            manifest,
            texts,
        };
        notices.check_references(identifiers)?;
        Ok(notices)
    }

    fn file_lists(&self) -> Result<Vec<&Vec<Value>>> {
        let mut lists = vec![array(&self.manifest, "deno_files")?];
        for group in ["sources", "crates"] {
            for item in array(&self.manifest, group)? {
                lists.push(array(item, "files")?);
            }
        }
        lists.push(array(&self.manifest, "spdx")?);
        Ok(lists)
    }

    fn check_references(&self, identifiers: &spdx::Identifiers) -> Result<()> {
        let mut used = BTreeSet::new();
        for list in self.file_lists()? {
            for file in list {
                let hash = string(file, "sha256")?;
                if !self.texts.contains_key(hash) {
                    return Err(format!(
                        "Deno notice manifest references missing text {hash}"
                    ));
                }
                used.insert(hash.to_owned());
            }
        }
        if let Some(unused) = self.texts.keys().find(|hash| !used.contains(*hash)) {
            return Err(format!("Deno notice text {unused} is not referenced"));
        }
        let standard: BTreeSet<&str> = array(&self.manifest, "spdx")?
            .iter()
            .map(|entry| string(entry, "id"))
            .collect::<Result<_>>()?;
        let available = |id: &str| standard.contains(id);
        let mut uncovered = Vec::new();
        for item in array(&self.manifest, "crates")? {
            let name = string(item, "name")?;
            let version = string(item, "version")?;
            if !array(item, "files")?.is_empty() {
                continue;
            }
            let declared = item["license"].as_str().unwrap_or_default();
            let covered = spdx::parse(&spdx::normalize(declared), identifiers)
                .is_ok_and(|expression| spdx::satisfiable(&expression, &available));
            if !covered {
                uncovered.push(format!("{name} {version} ({declared})"));
            }
        }
        if uncovered.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Deno crates without license files lack a standard text in the notice set:\n  {}",
                uncovered.join("\n  ")
            ))
        }
    }

    /// The aggregated notice. Identical texts are printed once.
    pub fn render(&self) -> Result<String> {
        let manifest = &self.manifest;
        let deno = &manifest["deno"];
        let rusty = &manifest["rusty_v8"];
        let rule = "=".repeat(78);
        let thin = "-".repeat(78);
        let mut printed: BTreeMap<String, String> = BTreeMap::new();
        let mut text = String::new();
        let mut file =
            |text: &mut String, label: String, path: &str, hash: &str| match printed.get(hash) {
                Some(first) => {
                    let _ = writeln!(text, "[{path}: identical to {first} above]\n");
                }
                None => {
                    let _ = writeln!(text, "[{path}]\n{}\n", self.texts[hash].trim_end());
                    printed.insert(hash.to_owned(), label);
                }
            };
        let _ = writeln!(
            text,
            "Third-party notices for the bundled Deno {version} executable\n\n\
Deno is MIT licensed. The deno executable also statically links V8, its\n\
third-party C/C++ libraries, the Rust standard library and the Rust crates\n\
listed below, each under its own license. These are the license files\n\
published in those sources at the exact revisions the release was built from.\n\n\
Deno release: {release}\n  executable SHA-256 {sha}\n  source tag {tag}, commit {commit}\n\
rusty_v8 (crate v8) {rv}: tag {rtag}, commit {rcommit}\n\
Cargo packages: `{command}` at that commit (Cargo.lock SHA-256 {lock}).\n",
            version = self.version,
            release = string(deno, "release")?,
            sha = self.executable_sha256,
            tag = string(deno, "tag")?,
            commit = string(deno, "commit")?,
            rv = string(rusty, "version")?,
            rtag = string(rusty, "tag")?,
            rcommit = string(rusty, "commit")?,
            command = string(&manifest["cargo"], "command")?,
            lock = string(&manifest["cargo"], "lock_sha256")?,
        );

        let _ = writeln!(text, "{rule}\nDeno {}\n{rule}", self.version);
        for entry in array(manifest, "deno_files")? {
            let path = string(entry, "path")?;
            let _ = writeln!(
                text,
                "{} (license {}; from {})",
                string(entry, "component")?,
                entry["license"].as_str().unwrap_or("see text"),
                string(entry, "source")?
            );
            file(
                &mut text,
                format!("deno {path}"),
                path,
                string(entry, "sha256")?,
            );
        }
        let _ = writeln!(
            text,
            "Deno's own workspace crates, all under Deno's MIT license above:\n  {}\n",
            array(&manifest["cargo"], "workspace_crates")?
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        );

        let _ = writeln!(
            text,
            "{rule}\nrusty_v8, V8, statically linked C/C++ libraries and the Rust standard library\n{rule}"
        );
        for source in array(manifest, "sources")? {
            let name = string(source, "name")?;
            let _ = writeln!(
                text,
                "{thin}\n{name}\nLocation: {}\nRepository: {} at commit {}\nLicense: {}\n",
                string(source, "location")?,
                string(source, "repository")?,
                string(source, "commit")?,
                source["license"]
                    .as_str()
                    .unwrap_or("as stated in the files below"),
            );
            for entry in array(source, "files")? {
                let path = string(entry, "path")?;
                file(
                    &mut text,
                    format!("{name} {path}"),
                    path,
                    string(entry, "sha256")?,
                );
            }
        }

        let crates = array(manifest, "crates")?;
        let _ = writeln!(
            text,
            "{rule}\nRust crates ({} packages from crates.io)\n{rule}\nEach entry reproduces the license files published with the crate, including\nvendored native sources. Crates that publish no license file are used under\ntheir declared license; the standard texts are at the end of this file.\n",
            crates.len()
        );
        for item in crates {
            let name = string(item, "name")?;
            let version = string(item, "version")?;
            let authors: Vec<&str> = array(item, "authors")?
                .iter()
                .filter_map(Value::as_str)
                .collect();
            let _ = writeln!(
                text,
                "{thin}\n{name} {version}\nLicense: {}\nAuthors: {}\nRepository: {}\ncrates.io SHA-256: {}\n",
                item["license"].as_str().unwrap_or("(license file below)"),
                if authors.is_empty() {
                    "(not declared)".to_owned()
                } else {
                    authors.join(", ")
                },
                item["repository"].as_str().unwrap_or("(not declared)"),
                item["checksum"].as_str().unwrap_or("(none)"),
            );
            let files = array(item, "files")?;
            if files.is_empty() {
                let _ = writeln!(
                    text,
                    "The published crate contains no license file. Its declared license is\nlisted above; the standard license text is included below. Authors metadata\nis not a copyright declaration.\n"
                );
            }
            for entry in files {
                let path = string(entry, "path")?;
                file(
                    &mut text,
                    format!("{name} {version} {path}"),
                    path,
                    string(entry, "sha256")?,
                );
            }
        }

        let _ = writeln!(
            text,
            "{rule}\nStandard license texts (SPDX License List 3.27.0)\n{rule}"
        );
        for entry in array(manifest, "spdx")? {
            let id = string(entry, "id")?;
            file(
                &mut text,
                format!("SPDX {id}"),
                &format!("{id} from {}", string(entry, "source")?),
                string(entry, "sha256")?,
            );
        }
        let _ = writeln!(
            text,
            "{rule}\nPresent in the pinned sources but not linked into this executable\n{rule}"
        );
        for line in array(manifest, "excluded")? {
            let _ = writeln!(text, "- {}", line.as_str().unwrap_or_default());
        }
        Ok(text)
    }

    /// Write the aggregated notice and the manifest into the bundle notices.
    pub fn install(&self, notices: &Path) -> Result<()> {
        let rendered = self.render()?;
        let target = notices.join(BUNDLED_NOTICES);
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&target, rendered).map_err(|e| e.to_string())?;
        fs::write(notices.join(BUNDLED_MANIFEST), &self.manifest_bytes).map_err(|e| e.to_string())
    }

    /// Nest the linked components under the Deno helper in a CycloneDX BOM.
    pub fn attach_to_sbom(&self, bom: &mut Value, identifiers: &spdx::Identifiers) -> Result<()> {
        let license = |declared: Option<&str>| {
            declared
                .map(spdx::normalize)
                .filter(|expression| spdx::parse(expression, identifiers).is_ok())
                .map(|expression| json!([{ "expression": expression }]))
        };
        let mut components = Vec::new();
        for source in array(&self.manifest, "sources")? {
            let key = string(source, "key")?;
            let commit = string(source, "commit")?;
            let mut component = json!({
                "type": "library",
                "bom-ref": format!("deno:source:{key}"),
                "name": string(source, "name")?,
                "version": commit,
                "externalReferences": [{
                    "type": "vcs",
                    "url": format!("{}#{commit}", string(source, "repository")?),
                }],
                "properties": [{ "name": "deadpan:linked-into", "value": "deno" }],
            });
            if let Some(licenses) = license(source["license"].as_str()) {
                component["licenses"] = licenses;
            }
            components.push(component);
        }
        for item in array(&self.manifest, "crates")? {
            let name = string(item, "name")?;
            let version = string(item, "version")?;
            let mut component = json!({
                "type": "library",
                "bom-ref": format!("deno:cargo:{name}@{version}"),
                "name": name,
                "version": version,
                "purl": format!("pkg:cargo/{name}@{version}"),
            });
            match license(item["license"].as_str()) {
                Some(licenses) => component["licenses"] = licenses,
                None => {
                    component["properties"] = json!([{
                        "name": "deadpan:declared-license",
                        "value": item["license"].as_str().unwrap_or("(license file only)"),
                    }]);
                }
            }
            if let Some(checksum) = item["checksum"].as_str() {
                component["hashes"] = json!([{ "alg": "SHA-256", "content": checksum }]);
            }
            components.push(component);
        }
        let helper = bom["components"]
            .as_array_mut()
            .and_then(|list| list.iter_mut().find(|c| c["bom-ref"] == SBOM_HELPER))
            .ok_or("the SBOM has no Deno helper component")?;
        if helper["version"] != self.version.as_str() {
            return Err(format!(
                "the SBOM's Deno {} differs from the notice set {}",
                helper["version"], self.version
            ));
        }
        helper["components"] = Value::Array(components);
        if let Some(properties) = helper["properties"].as_array_mut() {
            properties.push(json!({ "name": "deadpan:notices", "value": BUNDLED_NOTICES }));
        }
        Ok(())
    }
}

/// Check a bundle's Deno notices against the vendored set for the Deno it
/// ships. Empty means the bundle matches the reviewed vendored inventory;
/// source-level completeness is established when that inventory is updated.
pub fn audit_bundle(
    app: &Path,
    notice_sets: &Path,
    identifiers: &spdx::Identifiers,
) -> Vec<String> {
    let mut problems = Vec::new();
    let resources = app.join("Contents/Resources");
    let notices = resources.join("Notices");
    let helpers: Value = match fs::read(resources.join("helpers/manifest.json"))
        .map_err(|e| e.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
    {
        Ok(value) => value,
        Err(error) => return vec![format!("bundled helper manifest: {error}")],
    };
    let Some(deno) = helpers["helpers"]
        .as_array()
        .and_then(|list| list.iter().find(|helper| helper["name"] == "deno"))
    else {
        return vec!["the bundled helper manifest lists no Deno".into()];
    };
    let version = deno["version"].as_str().unwrap_or_default();
    let set = match set_directory(notice_sets, version) {
        Ok(set) => set,
        Err(error) => return vec![error],
    };
    if !set.join("manifest.json").is_file() {
        return vec![format!(
            "no vendored notice set for the bundled Deno {version} ({})",
            set.display()
        )];
    }
    let vendored = match DenoNotices::load(&set, identifiers) {
        Ok(vendored) => vendored,
        Err(error) => return vec![error],
    };
    if vendored.version != version {
        problems.push(format!(
            "the bundled Deno is {version} but the notice set is for Deno {}",
            vendored.version
        ));
    }
    if deno["upstream_sha256"].as_str() != Some(vendored.executable_sha256.as_str()) {
        problems.push(format!(
            "the bundled Deno upstream SHA-256 {} differs from the notice set's {}",
            deno["upstream_sha256"], vendored.executable_sha256
        ));
    }
    // Deno retains its exact upstream signed bytes. Check the executable,
    // rather than allowing the helper manifest alone to vouch for the pin.
    let executable = resources.join("helpers/deno").join(version).join("deno");
    match super::sha256_file(&executable) {
        Ok(hash) if hash == vendored.executable_sha256 => {}
        Ok(_) => problems
            .push("the bundled Deno executable does not match the notice set's SHA-256".into()),
        Err(error) => problems.push(format!("cannot check the bundled Deno executable: {error}")),
    }
    match (fs::read(notices.join(BUNDLED_NOTICES)), vendored.render()) {
        (Ok(bytes), Ok(expected)) => {
            if bytes != expected.as_bytes() {
                problems.push(format!(
                    "{BUNDLED_NOTICES} does not match the vendored Deno {} notice set",
                    vendored.version
                ));
            }
        }
        (Err(error), _) => problems.push(format!("{BUNDLED_NOTICES} is missing: {error}")),
        (_, Err(error)) => problems.push(error),
    }
    match fs::read(notices.join(BUNDLED_MANIFEST)) {
        Ok(bytes) if bytes == vendored.manifest_bytes => {}
        Ok(_) => problems.push(format!(
            "{BUNDLED_MANIFEST} differs from the vendored manifest"
        )),
        Err(error) => problems.push(format!("{BUNDLED_MANIFEST} is missing: {error}")),
    }
    let summary = fs::read_to_string(notices.join("THIRD_PARTY_NOTICES.txt")).unwrap_or_default();
    if !summary.contains(BUNDLED_NOTICES) {
        problems.push(format!(
            "THIRD_PARTY_NOTICES.txt does not reference {BUNDLED_NOTICES}"
        ));
    }
    // Preserve the same dependency inventory in the machine-readable SBOM.
    // A text notice alone cannot establish that the shipped BOM is complete.
    let bom: Result<Value> = fs::read(notices.join("sbom.cdx.json"))
        .map_err(|e| e.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()));
    match bom {
        Ok(bom) => {
            let helpers = bom["components"]
                .as_array()
                .map(|components| {
                    components
                        .iter()
                        .filter(|c| c["bom-ref"] == SBOM_HELPER)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if helpers.len() != 1 {
                problems.push("sbom.cdx.json must contain exactly one Deno helper".into());
            } else {
                let actual = helpers[0];
                let mut expected = json!({"components": [{
                    "bom-ref": SBOM_HELPER, "version": vendored.version, "properties": []
                }]});
                match vendored.attach_to_sbom(&mut expected, identifiers) {
                    Ok(()) => {
                        if actual["version"] != vendored.version
                            || actual["components"] != expected["components"][0]["components"]
                            || !actual["properties"].as_array().is_some_and(|properties| {
                                properties.contains(
                                    &json!({"name": "deadpan:notices", "value": BUNDLED_NOTICES}),
                                )
                            })
                            || !actual["hashes"].as_array().is_some_and(|hashes| {
                                hashes.contains(
                                &json!({"alg": "SHA-256", "content": vendored.executable_sha256})
                            )
                            })
                        {
                            problems.push(
                                "sbom.cdx.json Deno inventory differs from the vendored notice set"
                                    .into(),
                            );
                        }
                    }
                    Err(error) => problems.push(error),
                }
            }
        }
        Err(error) => problems.push(format!("cannot read sbom.cdx.json: {error}")),
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXECUTABLE: &str = "6cee2e06015a8ba81beb3c9958a61ff9483c09f21fb6215dc0b1f502031395db";

    fn identifiers() -> spdx::Identifiers {
        super::super::notices::spdx_identifiers(&super::super::workspace_root()).unwrap()
    }

    struct Fixture {
        root: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("xtask-deno-notices-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn sets(&self) -> PathBuf {
            self.root.join("notices")
        }

        fn set(&self) -> PathBuf {
            self.sets().join("deno-2.9.7")
        }

        fn text(&self, body: &str) -> String {
            let hash = sha256_hex(body.as_bytes());
            let directory = self.set().join("texts");
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join(format!("{hash}.txt")), body).unwrap();
            hash
        }

        /// A small but complete notice set.
        fn write_set(&self) -> Value {
            let deno = self.text("MIT License\nCopyright the Deno authors\n");
            let v8 = self.text("Copyright 2006-2011, the V8 project authors.\n");
            let crate_license = self.text("Copyright (c) Tokio Contributors\n");
            let mit = self.text("MIT standard text\n");
            let manifest = json!({
                "schema": 1,
                "deno": {
                    "version": "2.9.7", "tag": "v2.9.7", "commit": "0c07",
                    "release": "https://example.invalid/deno.zip",
                    "executable_sha256": EXECUTABLE,
                },
                "rusty_v8": { "version": "150.4.0", "tag": "v150.4.0", "commit": "5c15" },
                "cargo": { "command": "cargo tree", "lock_sha256": "00", "workspace_crates": ["deno 2.9.7"] },
                "deno_files": [{ "path": "LICENSE.md", "component": "Deno", "license": "MIT", "source": "s", "sha256": deno }],
                "sources": [{
                    "key": "v8", "name": "V8", "repository": "https://github.com/denoland/v8.git",
                    "commit": "ac1e", "location": "rusty_v8/v8", "license": null,
                    "files": [{ "path": "LICENSE", "source": "s", "sha256": v8 }],
                }],
                "crates": [
                    { "name": "tokio", "version": "1.47.1", "license": "MIT", "authors": [], "repository": null,
                      "checksum": "ab", "files": [{ "path": "LICENSE", "sha256": crate_license }] },
                    { "name": "bare", "version": "0.1.0", "license": "MIT/Apache-2.0", "authors": ["A"],
                      "repository": null, "checksum": null, "files": [] },
                ],
                "spdx": [{ "id": "MIT", "source": "s", "sha256": mit }],
                "excluded": ["partition_alloc: off"],
                "texts": [deno, v8, crate_license, mit],
            });
            self.write_manifest(&manifest);
            manifest
        }

        fn write_manifest(&self, manifest: &Value) {
            fs::write(
                self.set().join("manifest.json"),
                serde_json::to_vec_pretty(manifest).unwrap(),
            )
            .unwrap();
        }

        /// A minimal bundle whose Deno notices were installed from the set.
        fn app(&self, version: &str, upstream: &str) -> PathBuf {
            let app = self.root.join("Deadpan.app");
            let resources = app.join("Contents/Resources");
            fs::create_dir_all(resources.join("helpers")).unwrap();
            let executable = resources.join("helpers/deno").join(version).join("deno");
            fs::create_dir_all(executable.parent().unwrap()).unwrap();
            fs::write(executable, b"fixture Deno executable").unwrap();
            fs::write(
                resources.join("helpers/manifest.json"),
                serde_json::to_vec(&json!({ "schema": 1, "helpers": [
                    { "name": "yt-dlp", "version": "2026.08.19" },
                    { "name": "deno", "version": version, "upstream_sha256": upstream },
                ]}))
                .unwrap(),
            )
            .unwrap();
            let notices = resources.join("Notices");
            fs::create_dir_all(&notices).unwrap();
            fs::write(
                notices.join("THIRD_PARTY_NOTICES.txt"),
                format!("Deno 2.9.7\nSee {BUNDLED_NOTICES}.\n"),
            )
            .unwrap();
            let set = DenoNotices::load(&self.set(), &identifiers()).unwrap();
            set.install(&notices).unwrap();
            let mut bom = json!({"components": [{
                "bom-ref": SBOM_HELPER, "version": "2.9.7", "properties": [],
                "hashes": [{"alg": "SHA-256", "content": EXECUTABLE}],
            }]});
            set.attach_to_sbom(&mut bom, &identifiers()).unwrap();
            fs::write(
                notices.join("sbom.cdx.json"),
                serde_json::to_vec(&bom).unwrap(),
            )
            .unwrap();
            app
        }
    }

    #[test]
    fn loads_renders_and_deduplicates_a_complete_set() {
        let fixture = Fixture::new("load");
        fixture.write_set();
        let notices = DenoNotices::load(&fixture.set(), &identifiers()).unwrap();
        assert_eq!(notices.version, "2.9.7");
        let rendered = notices.render().unwrap();
        assert_eq!(rendered, notices.render().unwrap());
        for expected in [
            "Copyright 2006-2011, the V8 project authors.",
            "Copyright (c) Tokio Contributors",
            "Authors: A",
            "MIT standard text",
            "- partition_alloc: off",
        ] {
            assert!(rendered.contains(expected), "{expected}");
        }
        assert_eq!(rendered.matches("Copyright the Deno authors").count(), 1);
    }

    #[test]
    fn load_refuses_changed_missing_or_unlisted_texts() {
        let fixture = Fixture::new("tamper");
        let manifest = fixture.write_set();
        let texts = fixture.set().join("texts");
        let first = manifest["texts"][0].as_str().unwrap();
        let path = texts.join(format!("{first}.txt"));
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"changed\n").unwrap();
        let error = DenoNotices::load(&fixture.set(), &identifiers())
            .err()
            .unwrap();
        assert!(
            error.contains("does not match its recorded SHA-256"),
            "{error}"
        );
        fs::remove_file(&path).unwrap();
        assert!(DenoNotices::load(&fixture.set(), &identifiers()).is_err());
        fs::write(&path, &original).unwrap();
        fs::write(texts.join("extra.txt"), b"x").unwrap();
        let error = DenoNotices::load(&fixture.set(), &identifiers())
            .err()
            .unwrap();
        assert!(error.contains("unlisted"), "{error}");
        fs::remove_file(texts.join("extra.txt")).unwrap();
        assert!(DenoNotices::load(&fixture.set(), &identifiers()).is_ok());
    }

    #[test]
    fn load_refuses_dangling_references_and_uncovered_crates() {
        let fixture = Fixture::new("references");
        let manifest = fixture.write_set();
        let mut dangling = manifest.clone();
        dangling["crates"][0]["files"][0]["sha256"] = json!("f".repeat(64));
        fixture.write_manifest(&dangling);
        let error = DenoNotices::load(&fixture.set(), &identifiers())
            .err()
            .unwrap();
        assert!(error.contains("missing text"), "{error}");
        let mut uncovered = manifest.clone();
        uncovered["crates"][1]["license"] = json!("BSD-3-Clause");
        fixture.write_manifest(&uncovered);
        let error = DenoNotices::load(&fixture.set(), &identifiers())
            .err()
            .unwrap();
        assert!(error.contains("bare 0.1.0 (BSD-3-Clause)"), "{error}");
        let mut schema = manifest;
        schema["schema"] = json!(2);
        fixture.write_manifest(&schema);
        assert!(DenoNotices::load(&fixture.set(), &identifiers()).is_err());
    }

    #[test]
    fn audit_accepts_installed_notices_and_reports_each_defect() {
        let fixture = Fixture::new("audit");
        fixture.write_set();
        let app = fixture.app("2.9.7", EXECUTABLE);
        let ids = identifiers();
        assert_eq!(
            audit_bundle(&app, &fixture.sets(), &ids),
            Vec::<String>::new()
        );
        let notices = app.join("Contents/Resources/Notices");

        let bundled = notices.join(BUNDLED_NOTICES);
        let original = fs::read(&bundled).unwrap();
        fs::write(&bundled, b"truncated").unwrap();
        let problems = audit_bundle(&app, &fixture.sets(), &ids);
        assert!(problems[0].contains("does not match"), "{problems:?}");
        fs::remove_file(&bundled).unwrap();
        let problems = audit_bundle(&app, &fixture.sets(), &ids);
        assert!(problems[0].contains("is missing"), "{problems:?}");
        fs::write(&bundled, original).unwrap();

        fs::write(notices.join(BUNDLED_MANIFEST), b"{}").unwrap();
        let problems = audit_bundle(&app, &fixture.sets(), &ids);
        assert!(
            problems[0].contains("differs from the vendored manifest"),
            "{problems:?}"
        );
        fs::remove_file(notices.join(BUNDLED_MANIFEST)).unwrap();
        assert_eq!(audit_bundle(&app, &fixture.sets(), &ids).len(), 1);
    }

    #[test]
    fn audit_refuses_a_different_deno_or_a_changed_set() {
        let fixture = Fixture::new("version");
        let manifest = fixture.write_set();
        let ids = identifiers();
        // A bundle shipping another Deno has no matching notice set.
        let app = fixture.app("2.9.7", EXECUTABLE);
        let helpers = app.join("Contents/Resources/helpers/manifest.json");
        fs::write(
            &helpers,
            serde_json::to_vec(&json!({ "helpers": [
                { "name": "deno", "version": "2.9.8", "upstream_sha256": EXECUTABLE },
            ]}))
            .unwrap(),
        )
        .unwrap();
        let problems = audit_bundle(&app, &fixture.sets(), &ids);
        assert!(
            problems[0].contains("no vendored notice set for the bundled Deno 2.9.8"),
            "{problems:?}"
        );
        // The same version with a different executable.
        fs::write(
            &helpers,
            serde_json::to_vec(&json!({ "helpers": [
                { "name": "deno", "version": "2.9.7", "upstream_sha256": "0".repeat(64) },
            ]}))
            .unwrap(),
        )
        .unwrap();
        let problems = audit_bundle(&app, &fixture.sets(), &ids);
        assert!(
            problems.iter().any(|p| p.contains("upstream SHA-256")),
            "{problems:?}"
        );
        // A notice set directory whose manifest names another version.
        fs::write(
            &helpers,
            serde_json::to_vec(&json!({ "helpers": [
                { "name": "deno", "version": "2.9.7", "upstream_sha256": EXECUTABLE },
            ]}))
            .unwrap(),
        )
        .unwrap();
        let mut other = manifest;
        other["deno"]["version"] = json!("2.9.6");
        fixture.write_manifest(&other);
        let problems = audit_bundle(&app, &fixture.sets(), &ids);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("notice set is for Deno 2.9.6")),
            "{problems:?}"
        );
        assert!(set_directory(&fixture.sets(), "../x").is_err());
    }

    #[test]
    fn audit_checks_executable_bytes_and_the_sbom_inventory() {
        let fixture = Fixture::new("sbom-tamper");
        fixture.write_set();
        let app = fixture.app("2.9.7", EXECUTABLE);
        let ids = identifiers();
        let resources = app.join("Contents/Resources");
        fs::write(resources.join("helpers/deno/2.9.7/deno"), b"changed").unwrap();
        assert!(
            audit_bundle(&app, &fixture.sets(), &ids)
                .iter()
                .any(|p| p.contains("executable does not match"))
        );
        fs::write(
            resources.join("helpers/deno/2.9.7/deno"),
            b"fixture Deno executable",
        )
        .unwrap();
        let path = resources.join("Notices/sbom.cdx.json");
        let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut changed = original.clone();
        changed["components"][0]["components"]
            .as_array_mut()
            .unwrap()
            .pop();
        fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(
            audit_bundle(&app, &fixture.sets(), &ids)
                .iter()
                .any(|p| p.contains("Deno inventory differs"))
        );
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        assert!(audit_bundle(&app, &fixture.sets(), &ids).is_empty());
        fs::remove_file(path).unwrap();
        assert!(
            audit_bundle(&app, &fixture.sets(), &ids)
                .iter()
                .any(|p| p.contains("cannot read sbom"))
        );
    }

    #[test]
    fn sbom_nests_linked_components_under_the_helper() {
        let fixture = Fixture::new("sbom");
        fixture.write_set();
        let ids = identifiers();
        let notices = DenoNotices::load(&fixture.set(), &ids).unwrap();
        let mut bom = json!({ "components": [
            { "bom-ref": SBOM_HELPER, "name": "deno", "version": "2.9.7", "properties": [] },
        ]});
        notices.attach_to_sbom(&mut bom, &ids).unwrap();
        let nested = bom["components"][0]["components"].as_array().unwrap();
        assert_eq!(nested.len(), 3);
        assert_eq!(nested[1]["purl"], "pkg:cargo/tokio@1.47.1");
        assert_eq!(nested[2]["licenses"][0]["expression"], "MIT OR Apache-2.0");
        assert_eq!(
            bom["components"][0]["properties"][0]["value"],
            BUNDLED_NOTICES
        );
        let mut other = json!({ "components": [
            { "bom-ref": SBOM_HELPER, "version": "2.9.8", "properties": [] },
        ]});
        assert!(notices.attach_to_sbom(&mut other, &ids).is_err());
    }

    /// The vendored set for the pinned helper verifies and covers V8.
    #[test]
    fn vendored_set_for_the_pinned_deno_verifies() {
        let workspace = super::super::workspace_root();
        let sets = workspace.join("packaging/notices");
        let pinned =
            fs::read_to_string(workspace.join("crates/deadpan-cli/src/youtube/helpers.rs"))
                .unwrap();
        let notices =
            DenoNotices::load(&set_directory(&sets, "2.9.7").unwrap(), &identifiers()).unwrap();
        assert!(pinned.contains(&format!("\"{}\"", notices.executable_sha256)));
        let rendered = notices.render().unwrap();
        for component in [
            "V8",
            "ICU",
            "Abseil",
            "libc++",
            "simdutf",
            "Rust standard library",
            "tokio 1.47.1",
            "rustls 0.23.40",
            "aws-lc-sys",
        ] {
            assert!(rendered.contains(component), "{component}");
        }
    }
}
