use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;

use super::*;

fn helpers(directory: &Path) -> Helpers {
    Helpers {
        yt_dlp: directory.join("yt-dlp"),
        yt_dlp_version: "test".into(),
        deno: directory.join("deno dir/deno"),
        deno_version: "test".into(),
        pinned: None,
    }
}

fn video_format(id: &str, codec: &str, height: u32, fps: f64, protocol: &str) -> Value {
    serde_json::json!({
        "format_id": id, "ext": if codec.starts_with("avc1") { "mp4" } else { "webm" },
        "vcodec": codec, "acodec": "none", "width": height * 16 / 9, "height": height,
        "fps": fps, "tbr": f64::from(height), "protocol": protocol, "filesize": 1000,
    })
}

fn audio_format(id: &str, codec: &str, tbr: f64, language: i64) -> Value {
    serde_json::json!({
        "format_id": id, "ext": if codec.starts_with("mp4a") { "m4a" } else { "webm" },
        "vcodec": "none", "acodec": codec, "tbr": tbr, "asr": 44100,
        "protocol": "https", "language_preference": language, "filesize": 100,
    })
}

fn metadata(formats: Vec<Value>) -> Value {
    serde_json::json!({
        "_type": "video", "id": "Z4C82eyhwgU", "extractor_key": "Youtube",
        "title": "Caminandes\n2", "channel": "Blender", "channel_id": "UCSMOQeBJ2RAnuFungnQOxLg",
        "channel_url": "https://www.youtube.com/channel/UCSMOQeBJ2RAnuFungnQOxLg",
        "uploader_url": "http://insecure.example", "duration": 146, "live_status": "not_live",
        "thumbnail": "https://i.ytimg.com/vi/Z4C82eyhwgU/maxresdefault.jpg",
        "upload_date": "20141130", "license": null, "formats": formats,
    })
}

fn id() -> VideoId {
    VideoId::new("Z4C82eyhwgU").unwrap()
}

#[test]
fn arguments_disable_configuration_plugins_and_browser_sessions() {
    let directory = Path::new("/private/helpers");
    let helpers = helpers(directory);
    let arguments = metadata_arguments(&helpers, None, &id());
    let text: Vec<&str> = arguments.iter().map(|a| a.to_str().unwrap()).collect();
    for required in [
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--no-js-runtimes",
        "--no-cache-dir",
        "--no-cookies-from-browser",
        "--no-playlist",
        "--no-cookies",
        "--dump-single-json",
    ] {
        assert!(text.contains(&required), "{required}");
    }
    // The only enabled runtime is the pinned Deno, by absolute path, even
    // with spaces; clearing runtimes precedes it.
    let runtime = text.iter().position(|a| *a == "--js-runtimes").unwrap();
    assert!(text.iter().position(|a| *a == "--no-js-runtimes").unwrap() < runtime);
    assert_eq!(text[runtime + 1], "deno:/private/helpers/deno dir/deno");
    // The URL is canonical, after an end-of-options marker, and last.
    assert_eq!(
        &text[text.len() - 2..],
        ["--", "https://www.youtube.com/watch?v=Z4C82eyhwgU"]
    );
    assert!(
        !text
            .iter()
            .any(|a| a.contains("cookies-from-browser") && *a != "--no-cookies-from-browser")
    );

    let cookies = Path::new("/private/work/cookies.txt");
    let selection = select_formats(
        metadata(vec![
            video_format("137", "avc1.640028", 1080, 24.0, "https"),
            audio_format("140", "mp4a.40.2", 129.0, -1),
        ])["formats"]
            .as_array()
            .unwrap(),
    )
    .unwrap();
    let download = download_arguments(
        &helpers,
        Some(cookies),
        Path::new("/private/work/info.json"),
        &selection,
        99,
    );
    let text: Vec<&str> = download.iter().map(|a| a.to_str().unwrap()).collect();
    let after = |flag: &str| text[text.iter().position(|a| *a == flag).unwrap() + 1];
    assert_eq!(after("--cookies"), "/private/work/cookies.txt");
    assert!(!text.contains(&"--no-cookies"));
    assert_eq!(after("--load-info-json"), "/private/work/info.json");
    assert_eq!(after("--format"), "137,140");
    // Relative to the private download directory: no path enters a template.
    assert_eq!(after("--output"), "%(format_id)s.%(ext)s");
    assert!(text.contains(&"--quiet"));
    assert_eq!(after("--max-filesize"), "99");
    assert!(text.contains(&"--ignore-config") && text.contains(&"--no-plugin-dirs"));
}

#[test]
fn environment_is_private_and_complete() {
    let environment = environment(Path::new("/private/work"));
    assert_eq!(environment[OsStr::new("HOME")], "/private/work/home");
    assert_eq!(environment[OsStr::new("TMPDIR")], "/private/work/tmp");
    assert_eq!(
        environment[OsStr::new("XDG_CONFIG_HOME")],
        "/private/work/home/.config"
    );
    assert_eq!(environment[OsStr::new("PATH")], "/usr/bin:/bin");
    assert!(
        environment
            .keys()
            .all(|key| !key.to_str().unwrap().contains("PROXY"))
    );
}

#[test]
fn selection_prefers_admissible_highest_quality_streams() {
    let formats = vec![
        video_format("399", "av01.0.08M.08", 1080, 24.0, "https"),
        video_format("248", "vp9", 1080, 24.0, "https"),
        video_format("270", "avc1.640028", 1080, 24.0, "m3u8_native"),
        video_format("136", "avc1.64001f", 720, 24.0, "https"),
        video_format("137", "avc1.640028", 1080, 24.0, "https"),
        video_format("18", "avc1.42001E", 360, 24.0, "https"),
        audio_format("251", "opus", 139.0, -1),
        audio_format("140-drc", "mp4a.40.2", 129.6, -1),
        audio_format("140", "mp4a.40.2", 129.5, -1),
        audio_format("139", "mp4a.40.5", 48.0, -1),
        audio_format("140-dub", "mp4a.40.2", 200.0, -2),
    ];
    let selection = select_formats(&formats).unwrap();
    assert_eq!(selection.video.format_id, "137");
    assert_eq!(selection.audio.format_id, "140");

    let mut drm = video_format("299", "avc1.64002a", 1080, 60.0, "https");
    drm["has_drm"] = true.into();
    let selection = select_formats(&[drm, formats[4].clone(), formats[8].clone()]).unwrap();
    assert_eq!(selection.video.format_id, "137");

    let only_drc = select_formats(&[formats[4].clone(), formats[7].clone()]).unwrap();
    assert_eq!(only_drc.audio.format_id, "140-drc");
    let mut unsafe_id = formats[4].clone();
    unsafe_id["format_id"] = "137/../x".into();
    assert_eq!(
        select_formats(&[unsafe_id, formats[8].clone()])
            .unwrap_err()
            .code,
        "YouTubeFormatUnavailable"
    );
    assert_eq!(
        select_formats(&formats[..3]).unwrap_err().code,
        "YouTubeFormatUnavailable"
    );
    assert_eq!(
        select_formats(&[formats[4].clone(), formats[6].clone()])
            .unwrap_err()
            .code,
        "YouTubeFormatUnavailable"
    );
}

#[test]
fn metadata_refuses_playlists_live_streams_other_videos_and_long_videos() {
    let limits = ImportLimits::default();
    let formats = vec![
        video_format("137", "avc1.640028", 1080, 24.0, "https"),
        audio_format("140", "mp4a.40.2", 129.0, -1),
    ];
    let good = metadata(formats);
    let parsed = parse_metadata(good.to_string().as_bytes(), &id(), &limits).unwrap();
    assert_eq!(parsed.title, "Caminandes2");
    assert_eq!(parsed.author.as_deref(), Some("Blender"));
    assert_eq!(parsed.duration_seconds, 146.0);
    assert!(parsed.author_url.unwrap().starts_with("https://"));

    let refuse = |change: &dyn Fn(&mut Value), code: &str| {
        let mut value = good.clone();
        change(&mut value);
        assert_eq!(
            parse_metadata(value.to_string().as_bytes(), &id(), &limits)
                .unwrap_err()
                .code,
            code
        );
    };
    refuse(
        &|v| v["_type"] = "playlist".into(),
        "YouTubePlaylistRefused",
    );
    refuse(
        &|v| v["_type"] = "multi_video".into(),
        "YouTubePlaylistRefused",
    );
    refuse(
        &|v| v["live_status"] = "is_live".into(),
        "YouTubeLiveUnsupported",
    );
    refuse(
        &|v| v["live_status"] = "is_upcoming".into(),
        "YouTubeLiveUnsupported",
    );
    refuse(&|v| v["is_live"] = true.into(), "YouTubeLiveUnsupported");
    refuse(
        &|v| v["id"] = "aaaaaaaaaaa".into(),
        "YouTubeMetadataInvalid",
    );
    refuse(
        &|v| v["extractor_key"] = "Generic".into(),
        "YouTubeMetadataInvalid",
    );
    refuse(&|v| v["duration"] = (7 * 3600).into(), "YouTubeTooLong");
    refuse(&|v| v["duration"] = Value::Null, "YouTubeMetadataInvalid");
    refuse(
        &|v| v["formats"] = serde_json::json!([]),
        "YouTubeFormatUnavailable",
    );
    assert_eq!(
        parse_metadata(b"null", &id(), &limits).unwrap_err().code,
        "YouTubeVideoUnavailable"
    );
    assert_eq!(
        parse_metadata(b"{", &id(), &limits).unwrap_err().code,
        "YouTubeMetadataInvalid"
    );
}

#[test]
fn downloader_errors_map_to_actionable_codes() {
    for (stderr, code) in [
        (
            "ERROR: [youtube] x: Private video. Sign in if you've been granted access",
            "YouTubeVideoPrivate",
        ),
        (
            "ERROR: [youtube] x: Video unavailable. This video has been removed by the uploader",
            "YouTubeVideoUnavailable",
        ),
        (
            "ERROR: [youtube] x: This video is unavailable",
            "YouTubeVideoUnavailable",
        ),
        (
            "ERROR: [youtube] x: The uploader has not made this video available in your country",
            "YouTubeRegionRestricted",
        ),
        (
            "ERROR: [youtube] x: Sign in to confirm your age. This video may be inappropriate for some users.",
            "YouTubeAgeRestricted",
        ),
        (
            "ERROR: [youtube] x: Sign in to confirm you\u{2019}re not a bot.",
            "YouTubeRateLimited",
        ),
        (
            "ERROR: unable to download video data: HTTP Error 429: Too Many Requests",
            "YouTubeRateLimited",
        ),
        (
            "ERROR: [youtube] x: Join this channel to get access to members-only content",
            "YouTubeSignInRequired",
        ),
        (
            "ERROR: [youtube] x: This live event will begin in 3 hours.",
            "YouTubeLiveUnsupported",
        ),
        (
            "ERROR: [youtube] x: Requested format is not available",
            "YouTubeFormatUnavailable",
        ),
        (
            "ERROR: [youtube] x: Unable to download webpage: <urlopen error [Errno 8] nodename nor servname provided>",
            "YouTubeNetworkFailed",
        ),
        (
            "ERROR: unable to download video data: HTTP Error 403: Forbidden",
            "YouTubeDownloadFailed",
        ),
        (
            "ERROR: [youtube] x: Signature extraction failed: Some formats may be missing",
            "YouTubeExtractorFailed",
        ),
        (
            "WARNING: something\nERROR: [youtube] x: Unsupported URL: https://www.youtube.com/",
            "YouTubeExtractorFailed",
        ),
        (
            "Traceback (most recent call last):\n  boom",
            "DownloaderFailed",
        ),
        ("", "DownloaderFailed"),
    ] {
        let error = classify_failure(stderr);
        assert_eq!(error.code, code, "{stderr}");
        assert!(!error.message.is_empty());
    }
    let long = format!("ERROR: {}", "x".repeat(10_000));
    assert!(classify_failure(&long).message.len() < 1000);
}

#[test]
fn original_names_are_bounded_and_never_form_paths() {
    let id = id();
    assert_eq!(
        original_file_name("A/B: C\\D", &id),
        "A B C D [Z4C82eyhwgU].mp4"
    );
    assert_eq!(original_file_name("...", &id), "YouTube Z4C82eyhwgU.mp4");
    assert_eq!(original_file_name("\n", &id), "Original [Z4C82eyhwgU].mp4");
    for long in ["x".repeat(1000), "é".repeat(400), "\u{1F999}".repeat(300)] {
        let name = original_file_name(&long, &id);
        assert!(name.len() <= 255, "{}", name.len());
        assert!(name.ends_with(" [Z4C82eyhwgU].mp4"));
    }
    assert_eq!(
        original_file_name("Ünïcödé 名前", &id),
        "Ünïcödé 名前 [Z4C82eyhwgU].mp4"
    );
    assert!(!original_file_name("../../etc/passwd", &id).contains('/'));
}

#[test]
fn cookies_are_copied_owner_only_and_removed() {
    let workspace = Workspace::new().unwrap();
    let source = workspace.path().join("user-cookies.txt");
    fs::write(&source, "# Netscape HTTP Cookie File\n").unwrap();
    let cookies = PrivateCookies::copy(&source, &workspace).unwrap();
    let copy = cookies.path().to_owned();
    assert_ne!(copy, source);
    assert_eq!(
        fs::metadata(&copy).unwrap().permissions().mode() & 0o777,
        0o600
    );
    drop(cookies);
    assert!(!copy.exists());
    assert!(source.exists());
    assert_eq!(
        PrivateCookies::copy(workspace.path(), &workspace)
            .err()
            .unwrap()
            .code,
        "YouTubeCookiesInvalid"
    );
}

/// A stand-in yt-dlp: records its arguments and environment, then runs `body`.
fn stub(directory: &Path, body: &str) -> Helpers {
    let script = directory.join("yt-dlp");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{log}/arguments'\nenv > '{log}/environment'\n{body}\n",
            log = directory.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    helpers(directory)
}

fn import<'a>(package: &'a Path, helpers: &'a Helpers, cancelled: &'a AtomicBool) -> UrlImport<'a> {
    UrlImport {
        package,
        url: "https://youtu.be/Z4C82eyhwgU?si=tracking",
        cookies: None,
        helpers,
        media_worker: Path::new("/nonexistent/deadpan-media-worker"),
        limits: ImportLimits::default(),
        cancelled,
    }
}

#[test]
fn failures_before_transfer_create_no_project() {
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("never.deadpan");
    let cancelled = AtomicBool::new(false);
    let cases = [
        (
            "echo 'ERROR: [youtube] Z4C82eyhwgU: Private video' >&2; echo null; exit 1",
            "YouTubeVideoPrivate",
        ),
        (
            "echo 'ERROR: [youtube] Z4C82eyhwgU: Sign in to confirm your age' >&2; exit 1",
            "YouTubeAgeRestricted",
        ),
        (
            &format!(
                "cat <<'JSON'\n{}\nJSON",
                serde_json::json!({"_type": "playlist", "id": "Z4C82eyhwgU", "entries": []})
            ),
            "YouTubePlaylistRefused",
        ),
        ("echo 'not json'", "YouTubeMetadataInvalid"),
    ];
    for (body, code) in cases {
        let helpers = stub(directory.path(), body);
        let mut events = Vec::new();
        let error = create_from_url(&import(&package, &helpers, &cancelled), &mut |event| {
            events.push(event);
            Ok(())
        })
        .unwrap_err();
        assert!(
            matches!(&error, CliError::Import(error) if error.code == code),
            "{code}: {error}"
        );
        assert!(!package.exists());
    }
    // The stub saw the canonical URL, never the user's tracking parameters,
    // and ran with the private environment.
    let arguments = fs::read_to_string(directory.path().join("arguments")).unwrap();
    assert!(arguments.ends_with("--\nhttps://www.youtube.com/watch?v=Z4C82eyhwgU\n"));
    assert!(!arguments.contains("si=tracking"));
    let environment = fs::read_to_string(directory.path().join("environment")).unwrap();
    assert!(
        environment
            .lines()
            .any(|line| line.starts_with("HOME=") && line.contains("deadpan-youtube-"))
    );
    assert!(!environment.lines().any(|line| line.starts_with("USER=")));

    let helpers = stub(directory.path(), "sleep 30");
    let cancelled = AtomicBool::new(true);
    let started = Instant::now();
    let error =
        create_from_url(&import(&package, &helpers, &cancelled), &mut |_| Ok(())).unwrap_err();
    assert!(
        matches!(&error, CliError::Import(error) if error.code == "ImportCancelled"),
        "{error}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(!package.exists());
}

#[test]
fn transfer_failures_are_classified_and_create_no_project() {
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("never.deadpan");
    let good = metadata(vec![
        video_format("137", "avc1.640028", 1080, 24.0, "https"),
        audio_format("140", "mp4a.40.2", 129.0, -1),
    ]);
    let body = format!(
        "case \"$*\" in *--dump-single-json*) cat <<'JSON'\n{good}\nJSON\n;; *) echo 'ERROR: unable to download video data: HTTP Error 403: Forbidden' >&2; exit 1;; esac"
    );
    let helpers = stub(directory.path(), &body);
    let cancelled = AtomicBool::new(false);
    let mut events = Vec::new();
    let error = create_from_url(&import(&package, &helpers, &cancelled), &mut |event| {
        events.push(event);
        Ok(())
    })
    .unwrap_err();
    assert!(
        matches!(&error, CliError::Import(error) if error.code == "YouTubeDownloadFailed"),
        "{error}"
    );
    assert!(events.iter().any(|event| {
        event["event"] == "metadata"
            && event["selection"]["video"]["format_id"] == "137"
            && event["thumbnail_url"]
                .as_str()
                .is_some_and(|url| url.starts_with("https://"))
    }));
    assert!(!package.exists());

    // A successful exit that leaves no complete file is incomplete, not success.
    let body = format!(
        "case \"$*\" in *--dump-single-json*) cat <<'JSON'\n{good}\nJSON\n;; *) exit 0;; esac"
    );
    let helpers = stub(directory.path(), &body);
    let error =
        create_from_url(&import(&package, &helpers, &cancelled), &mut |_| Ok(())).unwrap_err();
    assert!(
        matches!(&error, CliError::Import(error) if error.code == "YouTubeDownloadIncomplete"),
        "{error}"
    );
    assert!(!package.exists());
}

#[test]
fn inspection_waits_for_confirmation_and_declining_transfers_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("never.deadpan");
    let good = metadata(vec![
        video_format("137", "avc1.640028", 1080, 24.0, "https"),
        audio_format("140", "mp4a.40.2", 129.0, -1),
    ]);
    // Any second run (a transfer) leaves evidence and fails.
    let body = format!(
        "case \"$*\" in *--dump-single-json*) cat <<'JSON'\n{good}\nJSON\n;; *) touch '{}/transferred'; exit 1;; esac",
        directory.path().display()
    );
    let helpers = stub(directory.path(), &body);
    let cookies = directory.path().join("cookies.txt");
    fs::write(&cookies, "# Netscape HTTP Cookie File\n").unwrap();
    let cancelled = AtomicBool::new(false);
    let mut events = Vec::new();
    let inspected = inspect(
        &Inspection {
            url: "https://www.youtube.com/watch?v=Z4C82eyhwgU&list=PLx",
            cookies: Some(&cookies),
            helpers: &helpers,
            limits: ImportLimits::default(),
            cancelled: &cancelled,
        },
        &mut |event| {
            events.push(event);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(inspected.video_id().as_str(), "Z4C82eyhwgU");
    assert_eq!(inspected.metadata().title, "Caminandes2");
    assert_eq!(inspected.metadata().selection.video.format_id, "137");
    assert_eq!(inspected.estimated_bytes(), Some(1100));
    assert_eq!(
        events
            .iter()
            .map(|event| event["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["fetching_metadata", "metadata"]
    );
    // The private workspace, with its cookie copy, lives until confirmation.
    let arguments = fs::read_to_string(directory.path().join("arguments")).unwrap();
    let copy = PathBuf::from(
        arguments
            .lines()
            .skip_while(|line| *line != "--cookies")
            .nth(1)
            .unwrap(),
    );
    assert!(copy.exists());
    let private = inspected.workspace.path().to_owned();
    drop(inspected);
    assert!(!copy.exists());
    assert!(!private.exists());
    assert!(!directory.path().join("transferred").exists());
    assert!(!package.exists());
    assert!(cookies.exists());
}

#[test]
fn confirming_an_inspection_refuses_an_existing_package_before_transfer() {
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("taken.deadpan");
    fs::create_dir(&package).unwrap();
    let good = metadata(vec![
        video_format("137", "avc1.640028", 1080, 24.0, "https"),
        audio_format("140", "mp4a.40.2", 129.0, -1),
    ]);
    let body = format!(
        "case \"$*\" in *--dump-single-json*) cat <<'JSON'\n{good}\nJSON\n;; *) touch '{}/transferred'; exit 1;; esac",
        directory.path().display()
    );
    let helpers = stub(directory.path(), &body);
    let cancelled = AtomicBool::new(false);
    let inspected = inspect(
        &Inspection {
            url: "https://youtu.be/Z4C82eyhwgU",
            cookies: None,
            helpers: &helpers,
            limits: ImportLimits::default(),
            cancelled: &cancelled,
        },
        &mut |_| Ok(()),
    )
    .unwrap();
    let error = download_and_create(
        inspected,
        &Acquisition {
            package: &package,
            alternatives: None,
            helpers: &helpers,
            media_worker: Path::new("/nonexistent/deadpan-media-worker"),
            limits: ImportLimits::default(),
            cancelled: &cancelled,
        },
        &mut |_| Ok(()),
    )
    .unwrap_err();
    assert!(matches!(error, CliError::Usage(message) if message.contains("already exists")));
    assert!(!directory.path().join("transferred").exists());
}

/// Gate G: hostile `--dump-single-json` output. yt-dlp is a pinned but
/// external program whose output describes remote content; parsing and
/// format selection must end in typed `ImportError`s within bounds.
#[test]
fn adversarial_metadata_json() {
    use deadpan_chaos::{Target, Verdict, fuzz};
    let formats = vec![
        video_format("137", "avc1.640028", 1080, 30.0, "https"),
        video_format("248", "vp9", 1080, 30.0, "https"),
        video_format("22", "avc1.64001F", 720, 60.0, "m3u8_native"),
        audio_format("140", "mp4a.40.2", 129.0, 10),
        audio_format("251", "opus", 140.0, -1),
    ];
    let seeds = vec![
        serde_json::to_vec(&metadata(formats.clone())).unwrap(),
        serde_json::to_vec(&metadata(formats[..2].to_vec())).unwrap(),
        serde_json::to_vec(&metadata(Vec::new())).unwrap(),
    ];
    let limits = ImportLimits::default();
    let report = fuzz(
        Target::json("cli-ytdlp-metadata").iterations(600),
        seeds,
        |input| match parse_metadata(input, &id(), &limits) {
            Ok(metadata) => {
                if metadata.id != id().as_str() {
                    return Err("accepted metadata for another video".into());
                }
                Ok(Verdict::Accepted)
            }
            Err(error) if error.code.is_empty() || error.message.is_empty() => {
                Err(format!("untyped import error {error:?}"))
            }
            Err(error) => Ok(Verdict::Rejected(format!(
                "{}: {}",
                error.code, error.message
            ))),
        },
    );
    report.assert_clean();
}
