//! End-to-end YouTube acquisition with a stand-in yt-dlp and the real worker.
//!
//! The stand-in emits fixture metadata, then "downloads" two split fixture
//! streams. Everything after it is real: format selection, stream-copy
//! assembly in the isolated worker, managed retention, stream qualification,
//! the single-Original baseline and private provenance.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use deadpan_cli::youtube::acquire::{ImportLimits, UrlImport, create_from_url};
use deadpan_cli::youtube::helpers::Helpers;
use deadpan_store::original_media::OriginalContentId;
use deadpan_store::original_provenance::FormatRole;
use deadpan_store::single_source::SingleSourceState;
use deadpan_store::{AccessMode, ProjectStore};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/remux")
        .join(name)
}

fn metadata() -> serde_json::Value {
    let bytes = |name| fs::metadata(fixture(name)).unwrap().len();
    serde_json::json!({
        "_type": "video", "id": "Z4C82eyhwgU", "extractor_key": "Youtube",
        "title": "Fixture: Gran Dillama / test", "channel": "Blender",
        "channel_id": "UCSMOQeBJ2RAnuFungnQOxLg",
        "channel_url": "https://www.youtube.com/channel/UCSMOQeBJ2RAnuFungnQOxLg",
        "duration": 2.5, "live_status": "not_live", "upload_date": "20141130",
        "thumbnail": "https://i.ytimg.com/vi/Z4C82eyhwgU/maxresdefault.jpg",
        "license": "Creative Commons Attribution license (reuse allowed)",
        "formats": [
            {"format_id": "248", "ext": "webm", "vcodec": "vp9", "acodec": "none",
             "width": 1920, "height": 1080, "fps": 24, "protocol": "https"},
            {"format_id": "137", "ext": "mp4", "vcodec": "avc1.64000d", "acodec": "none",
             "width": 320, "height": 180, "fps": 30, "tbr": 100.0, "protocol": "https",
             "filesize": bytes("video-fragmented.mp4")},
            {"format_id": "251", "ext": "webm", "vcodec": "none", "acodec": "opus",
             "tbr": 160.0, "protocol": "https"},
            {"format_id": "140", "ext": "m4a", "vcodec": "none", "acodec": "mp4a.40.2",
             "tbr": 128.0, "asr": 48000, "protocol": "https",
             "filesize": bytes("audio-fragmented.m4a")}
        ]
    })
}

/// Metadata on `--dump-single-json`; otherwise write both requested formats
/// to the relative `--output` template in the working directory, as yt-dlp
/// does for `-f 137,140`.
fn stand_in(directory: &Path) -> Helpers {
    let script = directory.join("yt-dlp");
    fs::write(
        &script,
        format!(
            r#"#!/bin/sh
printf '%s\n' "$@" > '{log}/arguments-'"$$"
case " $* " in
  *" --dump-single-json "*) cat <<'JSON'
{metadata}
JSON
  exit 0;;
esac
while [ $# -gt 0 ]; do
  case "$1" in
    --output) template="$2"; shift;;
    --load-info-json) info="$2"; shift;;
  esac
  shift
done
[ -s "$info" ] || {{ echo "ERROR: missing info" >&2; exit 1; }}
[ "$template" = '%(format_id)s.%(ext)s' ] || {{ echo "ERROR: absolute template" >&2; exit 1; }}
cp '{video}' 137.mp4
cp '{audio}' 140.m4a
"#,
            log = directory.display(),
            metadata = metadata(),
            video = fixture("video-fragmented.mp4").display(),
            audio = fixture("audio-fragmented.m4a").display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    Helpers {
        yt_dlp: script,
        yt_dlp_version: "2026.08.19".into(),
        deno: directory.join("deno"),
        deno_version: "2.9.7".into(),
        ejs_version: "0.8.0".into(),
        selection_note: None,
        pinned: None,
    }
}

#[test]
fn a_youtube_url_becomes_a_ready_one_original_project_with_provenance()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let helpers = stand_in(directory.path());
    let package = directory.path().join("from-url.deadpan");
    let cookies = directory.path().join("cookies.txt");
    fs::write(&cookies, "# Netscape HTTP Cookie File\n")?;
    let mut events = Vec::new();
    let created = create_from_url(
        &UrlImport {
            package: &package,
            url: "https://www.youtube.com/watch?v=Z4C82eyhwgU&list=PLx&index=2",
            cookies: Some(&cookies),
            helpers: &helpers,
            media_worker: Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
            limits: ImportLimits::default(),
            cancelled: &AtomicBool::new(false),
        },
        &mut |event| {
            events.push(event);
            Ok(())
        },
    )?;
    let names: Vec<_> = events
        .iter()
        .map(|e| e["event"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "fetching_metadata",
            "metadata",
            "assembling",
            "creating_project"
        ]
    );
    assert_eq!(events[1]["selection"]["video"]["format_id"], "137");
    assert_eq!(events[1]["selection"]["audio"]["format_id"], "140");

    // Both helper runs used the private cookie copy, never the user's file,
    // and the copy is gone.
    for entry in fs::read_dir(directory.path())? {
        let path = entry?.path();
        if path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("arguments-")
        {
            let arguments = fs::read_to_string(&path)?;
            assert!(arguments.contains("--ignore-config\n--no-plugin-dirs\n"));
            let copy = arguments
                .lines()
                .skip_while(|l| *l != "--cookies")
                .nth(1)
                .unwrap();
            assert_ne!(Path::new(copy), cookies);
            assert!(!Path::new(copy).exists());
        }
    }
    assert!(cookies.exists());

    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    store.validate()?;
    let Some(SingleSourceState::Ready {
        asset,
        baseline_revision,
        ..
    }) = store.single_source_state()?
    else {
        panic!("project is not Ready");
    };
    assert_eq!(asset, created.created.asset_id);
    let document = store.snapshot()?;
    assert_eq!(document.revision_id(), &baseline_revision);
    assert!(document.duration()?.frames() > 0);
    assert_eq!(document.presentation_basis().width, 320);
    let label = document.assets()[&asset].label.clone();
    assert_eq!(label, "Fixture: Gran Dillama / test");

    let content = OriginalContentId::new(
        created
            .created
            .content
            .trim_start_matches("blake3:")
            .to_owned(),
    )?;
    let record = store.original_record(&content)?.expect("retained original");
    assert!(record.managed());
    assert_eq!(
        record.label(),
        "Fixture Gran Dillama test [Z4C82eyhwgU].mp4"
    );
    let provenance = store.original_provenance(&content)?.expect("provenance");
    assert_eq!(provenance, created.provenance);
    assert_eq!(provenance.source_id, "Z4C82eyhwgU");
    assert_eq!(
        provenance.source_url,
        "https://www.youtube.com/watch?v=Z4C82eyhwgU"
    );
    assert_eq!(provenance.author.as_deref(), Some("Blender"));
    assert_eq!(provenance.formats.len(), 2);
    assert_eq!(provenance.formats[0].role, FormatRole::Video);
    assert_eq!(provenance.formats[0].format_id, "137");
    assert_eq!(provenance.formats[1].format_id, "140");
    assert!(
        provenance
            .helpers
            .iter()
            .any(|h| h.name == "yt-dlp" && h.version == "2026.08.19")
    );
    assert!(provenance.retrieved_at_unix_seconds > 1_700_000_000);
    // Provenance is operational metadata: nothing about it is in history.
    assert!(!document.to_json()?.contains("youtube.com"));
    Ok(())
}
