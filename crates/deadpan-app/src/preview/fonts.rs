//! Installed macOS interface fonts and text fallbacks. Font bytes stay local and
//! are never bundled: SF Pro and SF Mono become the primary families when the
//! system provides them, and egui's built-in fonts remain the portable fallback.

use std::{
    fs::File,
    io::Read,
    path::Path,
    sync::{Arc, OnceLock},
};

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use serde::Serialize;
use sha2::{Digest, Sha256};
use skrifa::MetadataProvider;

const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 48 * 1024 * 1024;
/// SF Pro's optical-size axis defaults to display sizes. Interface text uses
/// its smallest optical size, which has the wider spacing macOS uses for text.
const SF_TEXT_OPTICAL_SIZE: f32 = 17.0;
/// SF Mono's variable default is lighter than the system monospaced Regular.
const SF_MONO_REGULAR: f32 = 400.0;
const MAX_DIAGNOSTIC_CHARS: usize = 160;

// The report describes the fonts actually admitted at startup. The native replay
// can read this small value from egui context data without copying font bytes.
pub(super) const REPORT_KEY: &str = "deadpan-system-font-fallbacks";

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct Report {
    pub loaded: Vec<Provenance>,
    pub skipped: Vec<Skipped>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Provenance {
    pub path: String,
    pub face_index: u32,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Skipped {
    pub path: String,
    pub face_index: u32,
    pub reason: String,
}

struct Candidate<'a> {
    name: &'a str,
    path: &'a Path,
    index: u32,
    required: &'a str,
    role: Role,
}

/// A primary font leads one family; a fallback follows every family's
/// built-ins and supplies glyphs they lack.
#[derive(Clone, Copy)]
enum Role {
    Primary {
        monospace: bool,
        axis: [u8; 4],
        value: f32,
    },
    Fallback,
}

struct Loaded {
    fonts: Vec<(String, Role, Arc<FontData>)>,
    report: Report,
}

fn system() -> &'static Loaded {
    static SYSTEM: OnceLock<Loaded> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        load(
            &[
                Candidate {
                    name: "Deadpan SF Pro",
                    path: Path::new("/System/Library/Fonts/SFNS.ttf"),
                    index: 0,
                    required: "Deadpan 0123456789 ·…–⌘",
                    role: Role::Primary {
                        monospace: false,
                        axis: *b"opsz",
                        value: SF_TEXT_OPTICAL_SIZE,
                    },
                },
                Candidate {
                    name: "Deadpan SF Mono",
                    path: Path::new("/System/Library/Fonts/SFNSMono.ttf"),
                    index: 0,
                    required: "Deadpan 0123456789 :[]",
                    role: Role::Primary {
                        monospace: true,
                        axis: *b"wght",
                        value: SF_MONO_REGULAR,
                    },
                },
                Candidate {
                    name: "Deadpan Hiragino fallback",
                    path: Path::new("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc"),
                    index: 0,
                    required: "答え日本語",
                    role: Role::Fallback,
                },
                Candidate {
                    name: "Deadpan Unicode fallback",
                    path: Path::new("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
                    index: 0,
                    required: "答え日本語简体中文繁體한글한국어",
                    role: Role::Fallback,
                },
            ],
            MAX_TOTAL_BYTES,
        )
    })
}

pub(super) fn install(context: &egui::Context) {
    install_loaded(context, system());
}

fn install_loaded(context: &egui::Context, loaded: &Loaded) {
    let mut definitions = FontDefinitions::default();
    for (name, role, data) in &loaded.fonts {
        definitions.font_data.insert(name.clone(), Arc::clone(data));
        match role {
            Role::Primary { monospace, .. } => {
                let family = if *monospace {
                    FontFamily::Monospace
                } else {
                    FontFamily::Proportional
                };
                definitions
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, name.clone());
            }
            Role::Fallback => {
                for family in [FontFamily::Proportional, FontFamily::Monospace] {
                    definitions
                        .families
                        .entry(family)
                        .or_default()
                        .push(name.clone());
                }
            }
        }
    }
    context.set_fonts(definitions);
    context.data_mut(|data| data.insert_temp(egui::Id::new(REPORT_KEY), loaded.report.clone()));
}

fn load(candidates: &[Candidate<'_>], total_limit: usize) -> Loaded {
    let mut loaded = Loaded {
        fonts: Vec::new(),
        report: Report::default(),
    };
    let mut remaining = total_limit;
    for candidate in candidates {
        let admitted = read_font(candidate, remaining.min(MAX_FONT_BYTES));
        match admitted {
            Ok((data, provenance)) => {
                remaining -= provenance.bytes;
                loaded
                    .fonts
                    .push((candidate.name.into(), candidate.role, Arc::new(data)));
                loaded.report.loaded.push(provenance);
            }
            Err(reason) => loaded.report.skipped.push(Skipped {
                path: candidate.path.display().to_string(),
                face_index: candidate.index,
                reason: reason.chars().take(MAX_DIAGNOSTIC_CHARS).collect(),
            }),
        }
    }
    loaded
}

fn read_font(candidate: &Candidate<'_>, limit: usize) -> Result<(FontData, Provenance), String> {
    if limit == 0 {
        return Err("system font byte budget exhausted".into());
    }
    // Match the keymap reader: open without blocking on a special file, inspect
    // the descriptor, then enforce the cap again on the bytes actually read.
    let descriptor = rustix::fs::open(
        candidate.path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|error| format!("cannot open installed font: {error}"))?;
    let file = File::from(descriptor);
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("installed font must be a regular file".into());
    }
    if metadata.len() > limit as u64 {
        return Err("installed font exceeds byte budget".into());
    }
    let bytes = read_bounded(file, limit)?;
    validate(&bytes, candidate.index, candidate.required)?;
    let provenance = Provenance {
        path: candidate.path.display().to_string(),
        face_index: candidate.index,
        bytes: bytes.len(),
        sha256: Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    };
    let mut data = FontData::from_owned(bytes);
    data.index = candidate.index;
    if let Role::Primary { axis, value, .. } = candidate.role {
        data.tweak.coords = egui::epaint::text::VariationCoords::new([(axis, value)]);
    }
    Ok((data, provenance))
}

fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err("installed font exceeds byte budget".into());
    }
    Ok(bytes)
}

fn validate(bytes: &[u8], index: u32, required: &str) -> Result<(), String> {
    // epaint uses this same parser and otherwise panics on an invalid face.
    let face = skrifa::FontRef::from_index(bytes, index)
        .map_err(|error| format!("invalid installed font face: {error}"))?;
    let charmap = face.charmap();
    let outlines = face.outline_glyphs();
    for character in required.chars() {
        let glyph = charmap
            .map(character)
            .filter(|glyph| glyph.to_u32() != 0)
            .ok_or_else(|| format!("installed font lacks U+{:04X}", u32::from(character)))?;
        if outlines.get(glyph).is_none() {
            return Err(format!(
                "installed font lacks an outline for U+{:04X}",
                u32::from(character)
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
