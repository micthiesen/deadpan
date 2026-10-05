use super::scripted::Scripted;
use super::*;
use deadpan_store::single_source::SingleSourceState;
use deadpan_store::{AccessMode, ProjectStore};

const URL: &str = "https://youtu.be/Z4C82eyhwgU?si=share";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()
        .unwrap()
}

struct Setup {
    _scratch: tempfile::TempDir,
    root: PathBuf,
    downloader: Arc<Scripted>,
    jobs: Jobs,
}

fn setup(installed: bool) -> Setup {
    let scratch = tempfile::tempdir().unwrap();
    let documents = scratch.path().join("Documents");
    let library = ProjectLibrary::from_documents(documents.clone()).unwrap();
    let downloader = Arc::new(Scripted::new(fixture(), installed));
    let jobs = Jobs::new(downloader.clone(), Some(library), Arc::new(|| {}));
    Setup {
        _scratch: scratch,
        root: documents.join("Deadpan"),
        downloader,
        jobs,
    }
}

fn request() -> Request {
    Request {
        url: URL.into(),
        cookies: None,
    }
}

fn wait(jobs: &mut Jobs, what: &str, ready: impl Fn(&Jobs) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready(jobs) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}: {:?}",
            jobs.status()
        );
        jobs.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn packages(root: &Path) -> Vec<String> {
    std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn confirming(jobs: &Jobs) -> bool {
    matches!(jobs.status(), Status::Confirm { .. })
}

#[test]
fn confirmed_import_creates_one_ready_project_after_explicit_confirmation() {
    let Setup {
        _scratch,
        root,
        downloader,
        mut jobs,
    } = setup(true);
    jobs.start(request()).unwrap();
    assert!(jobs.status().busy());
    wait(&mut jobs, "confirmation", confirming);
    let Status::Confirm {
        preview,
        destination,
    } = jobs.status().clone()
    else {
        unreachable!()
    };
    assert_eq!(preview.metadata.title, "Caminandes 2: Gran Dillama");
    assert_eq!(destination, root.join("Caminandes 2 Gran Dillama.deadpan"));
    assert_eq!(
        destination_label(&destination),
        "Documents/Deadpan/Caminandes 2 Gran Dillama.deadpan"
    );
    // Nothing is transferred or created while waiting for the user.
    std::thread::sleep(Duration::from_millis(50));
    jobs.poll();
    assert!(confirming(&jobs));
    assert!(packages(&root).is_empty(), "{:?}", packages(&root));

    assert!(jobs.confirm());
    assert!(!jobs.confirm(), "confirmation is consumed once");
    wait(&mut jobs, "held transfer", |jobs| {
        matches!(
            jobs.status(),
            Status::Working(Stage::Downloading { downloaded, estimated: Some(total) })
                if *downloaded == total * 2 / 5
        )
    });
    downloader.release();
    wait(&mut jobs, "created project", |jobs| {
        matches!(jobs.status(), Status::Created(_))
    });
    assert!(!jobs.running());
    let created = jobs.take_created().unwrap();
    assert_eq!(created, destination);
    assert_eq!(jobs.take_created(), None);
    assert_eq!(*jobs.status(), Status::Idle);

    let store = ProjectStore::open(&created, AccessMode::ReadOnly).unwrap();
    store.validate().unwrap();
    assert!(matches!(
        store.single_source_state().unwrap(),
        Some(SingleSourceState::Ready { .. })
    ));
    let document = store.snapshot().unwrap();
    assert!(
        document
            .assets()
            .values()
            .any(|asset| asset.label == "Caminandes 2: Gran Dillama")
    );
    assert_eq!(
        downloader.requests.lock().unwrap().as_slice(),
        [request()],
        "one inspection, the user's URL unchanged"
    );
}

#[test]
fn declining_or_cancelling_a_transfer_creates_nothing() {
    let Setup {
        _scratch,
        root,
        downloader,
        mut jobs,
    } = setup(true);
    jobs.start(request()).unwrap();
    wait(&mut jobs, "confirmation", confirming);
    jobs.cancel();
    assert_eq!(*jobs.status(), Status::Cancelling);
    wait(&mut jobs, "declined", |jobs| !jobs.running());
    assert_eq!(*jobs.status(), Status::Cancelled);
    assert!(packages(&root).is_empty());
    jobs.dismiss();
    assert_eq!(*jobs.status(), Status::Idle);

    // Cancel while the transfer is held: nothing is left in the library.
    jobs.start(request()).unwrap();
    wait(&mut jobs, "confirmation", confirming);
    assert!(jobs.confirm());
    wait(&mut jobs, "held transfer", |jobs| {
        matches!(
            jobs.status(),
            Status::Working(Stage::Downloading { downloaded, .. }) if *downloaded > 0
        )
    });
    jobs.cancel();
    wait(&mut jobs, "cancelled transfer", |jobs| !jobs.running());
    assert_eq!(*jobs.status(), Status::Cancelled);
    assert!(packages(&root).is_empty(), "{:?}", packages(&root));
    assert_eq!(downloader.requests.lock().unwrap().len(), 2);
}

#[test]
fn missing_helpers_wait_for_an_explicit_install_then_continue() {
    let Setup {
        _scratch,
        downloader,
        mut jobs,
        ..
    } = setup(false);
    jobs.start(request()).unwrap();
    wait(&mut jobs, "install offer", |jobs| {
        *jobs.status() == Status::NeedsDownloader
    });
    // Nothing is installed without the explicit action.
    std::thread::sleep(Duration::from_millis(50));
    jobs.poll();
    assert!(!downloader.installed());
    assert_eq!(jobs.request(), Some(&request()));

    jobs.install().unwrap();
    assert!(matches!(jobs.status(), Status::Installing(_)));
    wait(&mut jobs, "inspection after install", confirming);
    assert!(downloader.installed());
    assert_eq!(downloader.requests.lock().unwrap().len(), 2);
    assert!(jobs.shutdown(Duration::from_secs(10)));
    assert_eq!(*jobs.status(), Status::Cancelled);
}

#[test]
fn failures_keep_their_codes_and_app_guidance() {
    let Setup {
        _scratch,
        root,
        downloader,
        mut jobs,
    } = setup(true);
    downloader.fail_next(Failure::new(
        "YouTubeAgeRestricted",
        "this video is age-restricted; retry with --cookies",
    ));
    jobs.start(request()).unwrap();
    wait(&mut jobs, "failure", |jobs| !jobs.running());
    let Status::Failed(failure) = jobs.status().clone() else {
        panic!("{:?}", jobs.status());
    };
    assert_eq!(failure.code, "YouTubeAgeRestricted");
    assert!(failure.needs_cookies());
    assert!(!failure.guidance().contains("--cookies"));
    assert!(packages(&root).is_empty());

    // Retrying with an explicit cookies file passes it through unchanged.
    let cookies = Request {
        url: URL.into(),
        cookies: Some(PathBuf::from("/explicit/cookies.txt")),
    };
    jobs.start(cookies.clone()).unwrap();
    wait(&mut jobs, "confirmation", confirming);
    assert_eq!(downloader.requests.lock().unwrap().last(), Some(&cookies));
    jobs.cancel();
    wait(&mut jobs, "cancelled", |jobs| !jobs.running());
}

#[test]
fn invalid_urls_are_refused_without_starting_a_job() {
    let Setup {
        _scratch,
        downloader,
        mut jobs,
        ..
    } = setup(true);
    jobs.start(Request {
        url: "https://www.youtube.com/playlist?list=PLx".into(),
        cookies: None,
    })
    .unwrap();
    assert!(!jobs.running());
    let Status::Failed(failure) = jobs.status() else {
        panic!("{:?}", jobs.status());
    };
    assert_eq!(failure.code, "YouTubePlaylistNeedsVideo");
    assert!(downloader.requests.lock().unwrap().is_empty());
}

#[test]
fn one_job_at_a_time_and_shutdown_drains_it() {
    let Setup {
        _scratch,
        root,
        mut jobs,
        ..
    } = setup(true);
    jobs.start(request()).unwrap();
    assert!(jobs.start(request()).is_err());
    assert!(jobs.install().is_err());
    wait(&mut jobs, "confirmation", confirming);
    assert!(jobs.confirm());
    // The transfer is held; shutdown cancels it and waits for teardown.
    assert!(jobs.shutdown(Duration::from_secs(10)));
    assert!(!jobs.running());
    assert!(packages(&root).is_empty());
}

#[test]
fn library_errors_map_to_stable_codes() {
    let failure = Failure::from(CliError::Import(deadpan_cli::youtube::ImportError::new(
        "YouTubeRegionRestricted",
        "not available",
    )));
    assert_eq!(failure.code, "YouTubeRegionRestricted");
    assert_eq!(
        Failure::from(CliError::Usage("/x.deadpan already exists".into())).code,
        "ProjectExists"
    );
    assert_eq!(
        Failure::from(CliError::Usage("other".into())).code,
        "DownloaderFailed"
    );
    assert!(Failure::new("DownloaderNotInstalled", "").needs_downloader());
    assert_eq!(duration_label(146.0), "2:26");
    assert_eq!(duration_label(3723.4), "1:02:03");
    let preview = scripted::preview();
    assert_eq!(
        picture_summary(&preview),
        "1920 × 1080 · 24 fps · H.264 (avc1.640028)"
    );
    assert_eq!(
        sound_summary(&preview),
        "AAC (mp4a.40.2) · 44.1 kHz · 130 kb/s"
    );
}

/// One real import through the app's job with the pinned helpers, network and
/// media worker. Set `DEADPAN_REAL_YOUTUBE_URL` to run it; it writes only to a
/// temporary library and needs `deadpan-cli downloader install` first.
#[test]
fn real_url_import_through_the_app_job_when_requested() {
    let Some(url) = std::env::var_os("DEADPAN_REAL_YOUTUBE_URL") else {
        return;
    };
    let worker = std::env::var_os("DEADPAN_MEDIA_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/deadpan-media-worker")
        })
        .canonicalize()
        .expect("build deadpan-media-worker or set DEADPAN_MEDIA_WORKER");
    let scratch = tempfile::tempdir().unwrap();
    let library = ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap();
    let mut jobs = Jobs::new(
        Arc::new(Pinned::new(None).with_media_worker(worker)),
        Some(library),
        Arc::new(|| {}),
    );
    let started = Instant::now();
    jobs.start(Request {
        url: url.to_string_lossy().into_owned(),
        cookies: None,
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30 * 60);
    let mut stages = Vec::new();
    loop {
        assert!(Instant::now() < deadline, "timed out: {:?}", jobs.status());
        jobs.poll();
        let label = match jobs.status() {
            Status::Working(Stage::Downloading { .. }) => "downloading".to_owned(),
            status => format!("{status:?}").chars().take(40).collect(),
        };
        if stages.last() != Some(&label) {
            eprintln!("{:>7.1}s {label}", started.elapsed().as_secs_f64());
            stages.push(label);
        }
        match jobs.status() {
            Status::Confirm {
                preview,
                destination,
            } => {
                eprintln!(
                    "confirm: {} · {} · {} -> {}",
                    preview.metadata.title,
                    picture_summary(preview),
                    sound_summary(preview),
                    destination.display()
                );
                assert!(jobs.confirm());
            }
            Status::Created(_) => break,
            Status::Failed(failure) => panic!("{failure:?}"),
            Status::NeedsDownloader => panic!("run `deadpan-cli downloader install` first"),
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let package = jobs.take_created().unwrap();
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    store.validate().unwrap();
    assert!(matches!(
        store.single_source_state().unwrap(),
        Some(SingleSourceState::Ready { .. })
    ));
    eprintln!(
        "created {} in {:.1}s",
        package.display(),
        started.elapsed().as_secs_f64()
    );
}

#[test]
fn unconfirmed_details_expire_and_ask_to_check_again() {
    let scratch = tempfile::tempdir().unwrap();
    let library = ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap();
    let downloader = Arc::new(Scripted::new(fixture(), true));
    let mut jobs = Jobs::new(downloader, Some(library), Arc::new(|| {}))
        .with_confirm_timeout(Duration::from_millis(100));
    jobs.start(request()).unwrap();
    wait(&mut jobs, "expiry", |jobs| !jobs.running());
    let Status::Failed(failure) = jobs.status() else {
        panic!("{:?}", jobs.status());
    };
    assert_eq!(failure.code, "YouTubeConfirmationExpired");
    assert!(failure.guidance().contains("check the video again"));
    assert!(packages(&scratch.path().join("Documents/Deadpan")).is_empty());
}

#[test]
fn a_destination_taken_during_the_download_moves_to_the_next_name() {
    let Setup {
        _scratch,
        root,
        downloader,
        mut jobs,
    } = setup(true);
    *downloader.take_during_build.lock().unwrap() = Some(Box::new(|path: &Path| {
        std::fs::create_dir(path).unwrap();
    }));
    jobs.start(request()).unwrap();
    wait(&mut jobs, "confirmation", confirming);
    assert!(jobs.confirm());
    downloader.release();
    wait(&mut jobs, "created", |jobs| {
        matches!(jobs.status(), Status::Created(_))
    });
    let created = jobs.take_created().unwrap();
    assert_eq!(created, root.join("Caminandes 2 Gran Dillama 2.deadpan"));
    let store = ProjectStore::open(&created, AccessMode::ReadOnly).unwrap();
    store.validate().unwrap();
}

/// Publishes its package even though cancellation arrived first, as a real
/// import does when Escape races the final rename.
struct LateCancel(PathBuf);

impl Downloader for LateCancel {
    fn install(&self, _: &AtomicBool, _: &mut dyn FnMut(InstallProgress)) -> Result<(), Failure> {
        Ok(())
    }

    fn import(
        &self,
        _: &Request,
        cancelled: &AtomicBool,
        _: &mut dyn FnMut(Stage),
        confirm: &mut dyn FnMut(&Preview) -> Result<PathBuf, Failure>,
    ) -> Result<PathBuf, Failure> {
        let package = confirm(&scripted::preview())?;
        while !cancelled.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let (package, _) = deadpan_cli::single_original::create_at_free_name(
            &package,
            &|_| None,
            &self.0,
            "late",
            &AtomicBool::new(false),
            |_, _| Ok(()),
        )?;
        Ok(package)
    }
}

#[test]
fn a_package_published_after_escape_still_opens_and_says_so() {
    let scratch = tempfile::tempdir().unwrap();
    let library = ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap();
    let mut jobs = Jobs::new(
        Arc::new(LateCancel(fixture())),
        Some(library),
        Arc::new(|| {}),
    );
    jobs.start(request()).unwrap();
    wait(&mut jobs, "confirmation", confirming);
    assert!(jobs.confirm());
    jobs.cancel();
    assert_eq!(*jobs.status(), Status::Cancelling);
    wait(&mut jobs, "published", |jobs| !jobs.running());
    assert!(matches!(jobs.status(), Status::Created(_)));
    assert!(jobs.completed_after_cancel());
    assert!(jobs.take_created().unwrap().exists());
}
