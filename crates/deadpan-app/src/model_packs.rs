//! Model packs in the native app: one background install, removal or discard
//! at a time, shared by every project.
//!
//! Packs live in one global directory. Nothing downloads without an explicit
//! Install (or Resume) after the pack's size and licenses are shown; packs
//! whose licenses require acceptance refuse to start until each is accepted.
//! Installation reuses the headless installer (resumable, verified download
//! or offline import, smoke test, activation) on its own thread, so it keeps
//! running when the project changes. Cancellation is cooperative and keeps
//! partial bytes for Resume. Quitting cancels and waits briefly.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use deadpan_cli::CliError;
use deadpan_models::packs::{
    ImportSource, InstallProgress, PackError, PackManifest, PackState, PackStore, approved_packs,
};

/// Space the installer keeps free beyond the remaining download.
pub const SPACE_MARGIN: u64 = deadpan_models::packs::FREE_SPACE_MARGIN;

/// What a job does to one pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Work {
    /// Download (resuming any partial bytes) or import from a folder or tar.
    Install {
        source: Option<PathBuf>,
    },
    Remove,
    Discard,
}

/// The visible step of a running job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Downloading or copying, each file verified against its hash.
    Transferring,
    SmokeTest,
    Activating,
    Removing,
    Discarding,
}

/// A running job as the panel shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub pack_id: String,
    pub work: Work,
    pub phase: Phase,
    pub progress: InstallProgress,
    pub cancelling: bool,
}

impl Job {
    pub fn label(&self) -> &'static str {
        match (self.phase, &self.work) {
            (Phase::Transferring, Work::Install { source: Some(_) }) => "Copying and verifying",
            (Phase::Transferring, _) => "Downloading and verifying",
            (Phase::SmokeTest, _) => "Testing the model on this Mac",
            (Phase::Activating, _) => "Activating",
            (Phase::Removing, _) => "Removing",
            (Phase::Discarding, _) => "Discarding the partial download",
        }
    }

    pub fn fraction(&self) -> f32 {
        self.progress.completed_bytes as f32 / self.progress.total_bytes.max(1) as f32
    }

    pub fn installing(&self) -> bool {
        matches!(self.work, Work::Install { .. })
    }
}

/// How the last job for a pack ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ending {
    Installed,
    Removed,
    Discarded,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub pack_id: String,
    pub ending: Ending,
}

enum Event {
    Progress(InstallProgress),
    Phase(Phase),
    Done(Ending),
}

struct Running {
    job: Job,
    cancel: Arc<AtomicBool>,
    events: mpsc::Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

/// What performs installs. Removal and discard always use the real store.
#[derive(Clone)]
pub enum Backend {
    /// `deadpan_cli::models::install_pack`: HTTPS download or offline import.
    Real,
    /// Deterministic installs for tests and UI replay; never downloads.
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted(Arc<scripted::Script>),
}

/// The pack states and free space last read from the store.
struct Snapshot {
    states: Vec<Result<PackState, String>>,
    free: Result<u64, String>,
    at: Instant,
}

pub struct Manager {
    root: Result<PathBuf, String>,
    backend: Backend,
    packs: Vec<PackManifest>,
    snapshot: Option<Snapshot>,
    running: Option<Running>,
    outcome: Option<Outcome>,
    /// Increments whenever a job finishes, so consumers re-check packs.
    changes: u64,
}

impl Manager {
    /// The global model directory and the real installer.
    pub fn new(backend: Backend) -> Self {
        Self {
            root: deadpan_cli::models::default_root().map_err(|error| error.to_string()),
            backend,
            packs: approved_packs(),
            snapshot: None,
            running: None,
            outcome: None,
            changes: 0,
        }
    }

    /// Use a private model directory, for example an empty one in replay.
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn set_root(&mut self, root: PathBuf) {
        self.root = Ok(root);
        self.snapshot = None;
    }

    #[cfg(feature = "ui-harness")]
    pub fn set_backend(&mut self, backend: Backend) {
        self.backend = backend;
    }

    pub fn root(&self) -> Result<&Path, &str> {
        self.root.as_deref().map_err(String::as_str)
    }

    pub fn store(&self) -> Option<PackStore> {
        self.root.clone().ok().map(PackStore::new)
    }

    pub fn packs(&self) -> &[PackManifest] {
        &self.packs
    }

    pub fn pack(&self, pack_id: &str) -> Option<&PackManifest> {
        self.packs.iter().find(|pack| pack.pack_id == pack_id)
    }

    /// Read every pack's state and the volume's free space. Cheap: one
    /// receipt and a stat per file, never a hash.
    pub fn refresh(&mut self) {
        let store = self.store();
        let states = self
            .packs
            .iter()
            .map(|pack| match &store {
                Some(store) => store.state(pack).map_err(|error| error.to_string()),
                None => Err(self.root.clone().err().unwrap_or_default()),
            })
            .collect();
        let free = match &self.root {
            Ok(root) => free_space(root),
            Err(error) => Err(error.clone()),
        };
        self.snapshot = Some(Snapshot {
            states,
            free,
            at: Instant::now(),
        });
    }

    /// Refresh when the snapshot is older than `age` (packs may change from
    /// the command line).
    pub fn refresh_if_stale(&mut self, age: Duration) {
        if self
            .snapshot
            .as_ref()
            .is_none_or(|snapshot| snapshot.at.elapsed() >= age)
        {
            self.refresh();
        }
    }

    pub fn state(&self, pack_id: &str) -> Option<&Result<PackState, String>> {
        let index = self.packs.iter().position(|pack| pack.pack_id == pack_id)?;
        self.snapshot.as_ref()?.states.get(index)
    }

    pub fn installed(&self, pack_id: &str) -> bool {
        matches!(self.state(pack_id), Some(Ok(PackState::Installed(_))))
    }

    pub fn free_space(&self) -> Option<&Result<u64, String>> {
        self.snapshot.as_ref().map(|snapshot| &snapshot.free)
    }

    pub fn job(&self) -> Option<&Job> {
        self.running.as_ref().map(|running| &running.job)
    }

    /// The running install of this pack, if any.
    pub fn installing(&self, pack_id: &str) -> Option<&Job> {
        self.job()
            .filter(|job| job.pack_id == pack_id && job.installing())
    }

    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    pub fn changes(&self) -> u64 {
        self.changes
    }

    /// Start one job. Installs require every license that needs acceptance
    /// to be in `accepted`; the installer checks this again.
    pub fn start(
        &mut self,
        pack_id: &str,
        work: Work,
        accepted: &BTreeSet<String>,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        if let Some(job) = self.job() {
            return Err(format!(
                "Another model job is running ({}). Wait for it or cancel it first.",
                job.label().to_lowercase()
            ));
        }
        let manifest = self
            .pack(pack_id)
            .cloned()
            .ok_or_else(|| format!("{pack_id} is not an approved model pack."))?;
        let store = self.store().ok_or_else(|| {
            format!(
                "Model storage is unavailable: {}",
                self.root.clone().err().unwrap_or_default()
            )
        })?;
        let accepted: Vec<String> = accepted.iter().cloned().collect();
        if matches!(work, Work::Install { .. }) {
            manifest
                .check_acceptance(&accepted)
                .map_err(|error| sentence(&error.to_string()))?;
        }
        let total = manifest.total_bytes();
        let completed = match store.state(&manifest) {
            Ok(PackState::Partial { bytes }) => bytes,
            _ => 0,
        };
        let phase = match work {
            Work::Install { .. } => Phase::Transferring,
            Work::Remove => Phase::Removing,
            Work::Discard => Phase::Discarding,
        };
        let job = Job {
            pack_id: pack_id.to_owned(),
            work: work.clone(),
            phase,
            progress: InstallProgress {
                completed_bytes: completed,
                total_bytes: total,
            },
            cancelling: false,
        };
        let (sender, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let backend = self.backend.clone();
        let repaint = Arc::new(repaint);
        let thread = std::thread::Builder::new()
            .name("deadpan-model-pack".into())
            .spawn(move || {
                let send = |event: Event| {
                    let _ = sender.send(event);
                    repaint();
                };
                let ending = run(
                    &backend,
                    &store,
                    &manifest,
                    &work,
                    &accepted,
                    &thread_cancel,
                    &send,
                );
                send(Event::Done(ending));
            })
            .map_err(|error| format!("The model job did not start: {error}"))?;
        self.outcome = None;
        self.running = Some(Running {
            job,
            cancel,
            events,
            thread: Some(thread),
        });
        Ok(())
    }

    /// Ask the running job to stop. Partial bytes stay for Resume.
    pub fn cancel(&mut self) {
        if let Some(running) = &mut self.running {
            running.cancel.store(true, Ordering::Release);
            running.job.cancelling = true;
        }
    }

    /// Apply the job's events. True when anything visible changed.
    pub fn poll(&mut self) -> bool {
        let Some(running) = &mut self.running else {
            return false;
        };
        // Observed before draining: a finished thread has sent everything.
        let thread_done = running.thread.as_ref().is_none_or(JoinHandle::is_finished);
        let mut changed = false;
        let mut finished = None;
        while let Ok(event) = running.events.try_recv() {
            changed = true;
            match event {
                Event::Progress(progress) => running.job.progress = progress,
                Event::Phase(phase) => running.job.phase = phase,
                Event::Done(ending) => {
                    finished = Some(ending);
                    break;
                }
            }
        }
        // A thread that ended without reporting still ends the job.
        if finished.is_none() && thread_done {
            finished = Some(Ending::Failed("The model job stopped unexpectedly.".into()));
        }
        if let Some(ending) = finished {
            let running = self.running.take().expect("running job");
            if let Some(thread) = running.thread {
                let _ = thread.join();
            }
            self.outcome = Some(Outcome {
                pack_id: running.job.pack_id,
                ending,
            });
            self.changes += 1;
            self.refresh();
            changed = true;
        }
        changed
    }

    /// Cancel and wait up to `timeout`, so a smoke-test worker is stopped
    /// before the process exits.
    pub fn shutdown(&mut self, timeout: Duration) {
        let Some(running) = &mut self.running else {
            return;
        };
        running.cancel.store(true, Ordering::Release);
        let Some(thread) = running.thread.take() else {
            return;
        };
        let deadline = Instant::now() + timeout;
        while !thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
    }
}

fn run(
    backend: &Backend,
    store: &PackStore,
    manifest: &PackManifest,
    work: &Work,
    accepted: &[String],
    cancel: &AtomicBool,
    send: &dyn Fn(Event),
) -> Ending {
    let source = match work {
        Work::Install { source } => source,
        Work::Remove => {
            return match store.remove(manifest) {
                Ok(()) => Ending::Removed,
                Err(error) => Ending::Failed(sentence(&error.to_string())),
            };
        }
        Work::Discard => {
            return match store.discard_partial(manifest) {
                Ok(()) => Ending::Discarded,
                Err(error) => Ending::Failed(pack_failure(&error)),
            };
        }
    };
    let result = match backend {
        Backend::Real => {
            let source = match source.as_deref().map(ImportSource::at).transpose() {
                Ok(source) => source,
                Err(error) => return Ending::Failed(pack_failure(&error)),
            };
            deadpan_cli::models::install_pack(
                store,
                manifest,
                accepted,
                source.as_ref(),
                cancel,
                |progress| send(Event::Progress(progress)),
                |phase| {
                    send(Event::Phase(match phase {
                        "smoke_test" => Phase::SmokeTest,
                        _ => Phase::Activating,
                    }))
                },
            )
            .map(|_| ())
        }
        #[cfg(any(test, feature = "ui-harness"))]
        Backend::Scripted(script) => {
            script.install(store, manifest, accepted, cancel, &|event| send(event))
        }
    };
    match result {
        Ok(()) => Ending::Installed,
        Err(_) if cancel.load(Ordering::Acquire) => Ending::Cancelled,
        Err(CliError::ModelPack(PackError::Cancelled)) => Ending::Cancelled,
        Err(CliError::ModelPack(error)) => Ending::Failed(pack_failure(&error)),
        Err(error) => Ending::Failed(sentence(&error.to_string())),
    }
}

/// A pack error in app terms.
pub fn pack_failure(error: &PackError) -> String {
    match error {
        PackError::Space {
            required,
            available,
        } => format!(
            "Not enough free space: {} needed, {} available on the models volume.",
            format_bytes(*required),
            format_bytes(*available)
        ),
        PackError::Busy => {
            "Another installation of this pack is running, perhaps from the command line. Try again when it finishes.".into()
        }
        error => sentence(&error.to_string()),
    }
}

fn sentence(text: &str) -> String {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    let mut sentence: String = first.to_uppercase().chain(characters).collect();
    if !sentence.ends_with('.') {
        sentence.push('.');
    }
    sentence
}

/// Free bytes on the volume that holds (or will hold) `root`.
fn free_space(root: &Path) -> Result<u64, String> {
    let existing = root
        .ancestors()
        .find(|path| path.exists())
        .ok_or_else(|| format!("{} has no existing folder", root.display()))?;
    deadpan_models::packs::available_space(existing).map_err(|error| error.to_string())
}

/// Decimal sizes as macOS shows them: 149 MB, 36.15 GB.
pub fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1e6;
    const GB: f64 = 1e9;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.2} GB", value / GB)
    } else if value >= 10.0 * MB {
        format!("{:.0} MB", value / MB)
    } else if value >= 1000.0 {
        format!("{:.1} MB", value / MB)
    } else {
        format!("{bytes} bytes")
    }
}

/// What the Install control says and how much it still downloads.
pub fn install_label(manifest: &PackManifest, state: Option<&PackState>) -> String {
    match state {
        Some(PackState::Partial { bytes }) => format!(
            "Resume · {} left",
            format_bytes(manifest.total_bytes().saturating_sub(*bytes))
        ),
        _ => format!("Install · {}", format_bytes(manifest.total_bytes())),
    }
}

/// Bytes a download still needs.
pub fn remaining(manifest: &PackManifest, state: Option<&PackState>) -> u64 {
    match state {
        Some(PackState::Installed(_)) => 0,
        Some(PackState::Partial { bytes }) => manifest.total_bytes().saturating_sub(*bytes),
        _ => manifest.total_bytes(),
    }
}

/// The refusal shown instead of starting a download that cannot fit.
pub fn space_refusal(remaining: u64, free: u64) -> Option<String> {
    let required = remaining.saturating_add(SPACE_MARGIN);
    (free < required).then(|| {
        format!(
            "Not enough free space: the download needs {} ({} plus {} working space) and {} is free. Free space on the models volume, or install from a folder on another disk.",
            format_bytes(required),
            format_bytes(remaining),
            format_bytes(SPACE_MARGIN),
            format_bytes(free)
        )
    })
}

/// Why Install (or Resume) is not available, or None when it is.
pub fn install_blocker(
    manifest: &PackManifest,
    accepted: &BTreeSet<String>,
    state: Option<&PackState>,
    free: Option<u64>,
    busy: bool,
) -> Option<String> {
    if matches!(state, Some(PackState::Installed(_))) {
        return Some("Installed.".into());
    }
    if busy {
        return Some("Another model job is running.".into());
    }
    let missing: Vec<&str> = manifest
        .licenses
        .iter()
        .filter(|license| license.acceptance_required && !accepted.contains(&license.id))
        .map(|license| license.title.as_str())
        .collect();
    if !missing.is_empty() {
        return Some(format!(
            "Accept {} to install.",
            match missing.as_slice() {
                [one] => format!("the {one}"),
                _ => "both licenses above".to_owned(),
            }
        ));
    }
    free.and_then(|free| space_refusal(remaining(manifest, state), free))
}

#[cfg(any(test, feature = "ui-harness"))]
pub mod scripted {
    //! Deterministic installs for tests and replay. Progress advances over
    //! several steps and honors cancellation; a cancelled run leaves sparse
    //! partial files so the store reports Resume, and a completed run
    //! materializes sparse files and a receipt the real store accepts.

    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    #[derive(Clone, Debug)]
    pub enum Finish {
        Complete,
        /// Stop with the real error text of this pack error.
        Fail(String),
        /// Wait at the last step until cancelled.
        WaitForCancel,
    }

    #[derive(Clone, Debug)]
    pub struct Run {
        pub steps: u32,
        pub interval: Duration,
        pub finish: Finish,
    }

    /// One run per install start, in order.
    pub struct Script {
        runs: Mutex<VecDeque<Run>>,
    }

    impl Script {
        pub fn new(runs: impl IntoIterator<Item = Run>) -> Self {
            Self {
                runs: Mutex::new(runs.into_iter().collect()),
            }
        }

        pub(super) fn install(
            &self,
            store: &PackStore,
            manifest: &PackManifest,
            accepted: &[String],
            cancel: &AtomicBool,
            send: &dyn Fn(Event),
        ) -> Result<(), CliError> {
            manifest.check_acceptance(accepted)?;
            let run = self
                .runs
                .lock()
                .map_err(|_| CliError::Usage("scripted install lock poisoned".into()))?
                .pop_front()
                .ok_or_else(|| CliError::Usage("no scripted model install remains".into()))?;
            let total = manifest.total_bytes();
            let start = match store.state(manifest)? {
                PackState::Partial { bytes } => bytes,
                _ => 0,
            };
            // A run that will not complete stops halfway through what is left.
            let end = if matches!(run.finish, Finish::Complete) {
                total
            } else {
                start + (total - start) / 2
            };
            let steps = u64::from(run.steps.max(1));
            let mut completed = start;
            for step in 1..=steps {
                if cancel.load(Ordering::Acquire) {
                    partial(store, manifest, completed)?;
                    return Err(PackError::Cancelled.into());
                }
                std::thread::sleep(run.interval);
                completed = start + (end - start) * step / steps;
                send(Event::Progress(InstallProgress {
                    completed_bytes: completed,
                    total_bytes: total,
                }));
            }
            match run.finish {
                Finish::Fail(message) => {
                    partial(store, manifest, completed)?;
                    Err(CliError::Usage(message))
                }
                Finish::WaitForCancel => {
                    while !cancel.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    partial(store, manifest, completed)?;
                    Err(PackError::Cancelled.into())
                }
                Finish::Complete => {
                    send(Event::Phase(Phase::SmokeTest));
                    std::thread::sleep(run.interval);
                    send(Event::Phase(Phase::Activating));
                    install(store, manifest, accepted)?;
                    Ok(())
                }
            }
        }
    }

    fn staging(store: &PackStore, manifest: &PackManifest) -> PathBuf {
        store
            .root()
            .join(".staging")
            .join(format!("{}-{}", manifest.pack_id, manifest.pack_version))
    }

    /// Sparse `.part` files holding `bytes` in manifest order.
    fn partial(store: &PackStore, manifest: &PackManifest, bytes: u64) -> std::io::Result<()> {
        let staging = staging(store, manifest);
        let mut left = bytes;
        for file in &manifest.files {
            let size = left.min(file.bytes);
            left -= size;
            let path = staging.join(format!("{}.part", file.name));
            if size == 0 {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::File::create(&path)?.set_len(size)?;
        }
        Ok(())
    }

    /// A sparse installed copy and receipt in the format `installed` accepts.
    pub fn install(
        store: &PackStore,
        manifest: &PackManifest,
        accepted: &[String],
    ) -> std::io::Result<()> {
        let directory = store
            .root()
            .join(&manifest.pack_id)
            .join(&manifest.pack_version);
        for file in &manifest.files {
            let path = directory.join(&file.name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::File::create(&path)?.set_len(file.bytes)?;
        }
        let receipt = serde_json::json!({
            "pack_id": manifest.pack_id,
            "pack_version": manifest.pack_version,
            "files": manifest.files.iter().map(|file| serde_json::json!({
                "name": file.name, "sha256": file.sha256, "bytes": file.bytes,
            })).collect::<Vec<_>>(),
            "accepted_licenses": accepted,
            "origin": "download",
        });
        std::fs::write(directory.join("receipt.json"), receipt.to_string())?;
        match std::fs::remove_dir_all(staging(store, manifest)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::scripted::{Finish, Run, Script};
    use super::*;

    const BRIDGE: &str = "ltx-2.3-q4-bridge";
    const WHISPER: &str = "whisper-base-en";

    fn manifest(id: &str) -> PackManifest {
        deadpan_models::packs::approved_pack(id).unwrap()
    }

    fn accepted(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn sizes_and_labels_show_remaining_bytes_when_partial() {
        let bridge = manifest(BRIDGE);
        assert_eq!(format_bytes(bridge.total_bytes()), "36.15 GB");
        assert_eq!(format_bytes(manifest(WHISPER).total_bytes()), "149 MB");
        assert_eq!(format_bytes(885_098), "0.9 MB");
        assert_eq!(install_label(&bridge, None), "Install · 36.15 GB");
        assert_eq!(
            install_label(&bridge, Some(&PackState::Absent)),
            "Install · 36.15 GB"
        );
        let partial = PackState::Partial {
            bytes: 12_000_000_000,
        };
        assert_eq!(
            install_label(&bridge, Some(&partial)),
            format!(
                "Resume · {} left",
                format_bytes(bridge.total_bytes() - 12_000_000_000)
            )
        );
        assert_eq!(
            remaining(&bridge, Some(&partial)),
            bridge.total_bytes() - 12_000_000_000
        );
    }

    #[test]
    fn install_waits_for_every_required_license() {
        let bridge = manifest(BRIDGE);
        let free = Some(u64::MAX);
        let none = install_blocker(&bridge, &accepted(&[]), None, free, false).unwrap();
        assert!(none.contains("both licenses"), "{none}");
        let one = install_blocker(&bridge, &accepted(&["ltx-2"]), None, free, false).unwrap();
        assert_eq!(one, "Accept the Gemma Terms of Use to install.");
        assert_eq!(
            install_blocker(&bridge, &accepted(&["ltx-2", "gemma"]), None, free, false),
            None
        );
        // MIT needs no acceptance.
        assert_eq!(
            install_blocker(&manifest(WHISPER), &accepted(&[]), None, free, false),
            None
        );
        assert!(
            install_blocker(&bridge, &accepted(&["ltx-2", "gemma"]), None, free, true).is_some()
        );
    }

    #[test]
    fn insufficient_space_refuses_with_both_amounts() {
        let bridge = manifest(BRIDGE);
        let all = accepted(&["ltx-2", "gemma"]);
        let refusal = install_blocker(&bridge, &all, None, Some(10_000_000_000), false).unwrap();
        assert!(refusal.starts_with("Not enough free space"), "{refusal}");
        assert!(refusal.contains("10.00 GB is free"), "{refusal}");
        assert!(refusal.contains("36.15 GB plus"), "{refusal}");
        // A partial download needs only its remaining bytes plus the margin.
        let partial = PackState::Partial {
            bytes: bridge.total_bytes() - 1_000_000_000,
        };
        assert_eq!(
            install_blocker(&bridge, &all, Some(&partial), Some(2_000_000_000), false),
            None
        );
        assert!(space_refusal(1_000_000_000, 1_000_000_000 + SPACE_MARGIN - 1).is_some());
        assert!(space_refusal(1_000_000_000, 1_000_000_000 + SPACE_MARGIN).is_none());
        let space = pack_failure(&PackError::Space {
            required: 40_000_000_000,
            available: 3_500_000_000,
        });
        assert_eq!(
            space,
            "Not enough free space: 40.00 GB needed, 3.50 GB available on the models volume."
        );
    }

    fn wait(manager: &mut Manager) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while manager.job().is_some() {
            manager.poll();
            assert!(Instant::now() < deadline, "scripted job did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn scripted_install_cancels_resumes_and_installs_in_a_private_root() {
        let root = tempfile::tempdir().unwrap();
        let script = Script::new([
            Run {
                steps: 4,
                interval: Duration::from_millis(1),
                finish: Finish::WaitForCancel,
            },
            Run {
                steps: 4,
                interval: Duration::from_millis(1),
                finish: Finish::Complete,
            },
        ]);
        let mut manager = Manager::new(Backend::Scripted(Arc::new(script)));
        manager.set_root(root.path().to_path_buf());
        manager.refresh();
        assert!(matches!(manager.state(BRIDGE), Some(Ok(PackState::Absent))));
        let refused = manager
            .start(
                BRIDGE,
                Work::Install { source: None },
                &accepted(&["ltx-2"]),
                || {},
            )
            .unwrap_err();
        assert!(refused.contains("Gemma"), "{refused}");
        assert!(manager.job().is_none());

        let all = accepted(&["ltx-2", "gemma"]);
        manager
            .start(BRIDGE, Work::Install { source: None }, &all, || {})
            .unwrap();
        assert!(
            manager
                .start(WHISPER, Work::Install { source: None }, &all, || {})
                .unwrap_err()
                .contains("Another model job")
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while manager
            .job()
            .is_some_and(|job| job.progress.completed_bytes == 0)
        {
            manager.poll();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        manager.cancel();
        wait(&mut manager);
        assert_eq!(manager.outcome().unwrap().ending, Ending::Cancelled);
        let Some(Ok(PackState::Partial { bytes })) = manager.state(BRIDGE) else {
            panic!("partial bytes kept: {:?}", manager.state(BRIDGE));
        };
        assert!(*bytes > 0);
        assert!(
            install_label(
                &manifest(BRIDGE),
                manager.state(BRIDGE).unwrap().as_ref().ok()
            )
            .starts_with("Resume")
        );

        let changes = manager.changes();
        manager
            .start(BRIDGE, Work::Install { source: None }, &all, || {})
            .unwrap();
        wait(&mut manager);
        assert_eq!(manager.outcome().unwrap().ending, Ending::Installed);
        assert!(manager.installed(BRIDGE));
        assert!(manager.changes() > changes);
        let store = manager.store().unwrap();
        let installed = store.installed(&manifest(BRIDGE)).unwrap();
        assert!(installed.is_some());

        manager.start(BRIDGE, Work::Remove, &all, || {}).unwrap();
        wait(&mut manager);
        assert_eq!(manager.outcome().unwrap().ending, Ending::Removed);
        assert!(matches!(manager.state(BRIDGE), Some(Ok(PackState::Absent))));
    }

    #[test]
    fn scripted_failure_is_reported_and_discard_clears_partial_bytes() {
        let root = tempfile::tempdir().unwrap();
        let script = Script::new([Run {
            steps: 2,
            interval: Duration::from_millis(1),
            finish: Finish::Fail("The offline source lacks 2 files.".into()),
        }]);
        let mut manager = Manager::new(Backend::Scripted(Arc::new(script)));
        manager.set_root(root.path().to_path_buf());
        manager
            .start(
                WHISPER,
                Work::Install { source: None },
                &accepted(&[]),
                || {},
            )
            .unwrap();
        wait(&mut manager);
        assert_eq!(
            manager.outcome().unwrap().ending,
            Ending::Failed("The offline source lacks 2 files.".into())
        );
        assert!(matches!(
            manager.state(WHISPER),
            Some(Ok(PackState::Partial { .. }))
        ));
        manager
            .start(WHISPER, Work::Discard, &accepted(&[]), || {})
            .unwrap();
        wait(&mut manager);
        assert_eq!(manager.outcome().unwrap().ending, Ending::Discarded);
        assert!(matches!(
            manager.state(WHISPER),
            Some(Ok(PackState::Absent))
        ));
    }
}
