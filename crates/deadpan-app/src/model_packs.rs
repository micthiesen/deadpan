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
//!
//! The same job slot applies signed updates and rollbacks (docs/UPDATES.md):
//! a signed model-pack update installs its version beside the current one and
//! is selected only after its smoke test; a signed downloader update installs
//! a new helper set under the managed root and is selected only after the
//! real probe. Rollback selects the previous version again; nothing is
//! deleted.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use deadpan_cli::CliError;
use deadpan_cli::youtube::helpers::{DENO, HelperSource, YT_DLP};
use deadpan_cli::youtube::updates as downloader_updates;
use deadpan_models::packs::updates::PackUpdate;
use deadpan_models::packs::{
    ImportSource, InstallProgress, PackError, PackManifest, PackState, PackStore, approved_packs,
};
use deadpan_models::updates::{SignedManifest, TrustedKey, UpdateKind, trusted_keys};

/// The job target of downloader updates and rollbacks.
pub const DOWNLOADER: &str = "downloader";

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
    /// Remove an installed version other than the selected one.
    RemoveVersion(String),
    Discard,
    /// Apply one reviewed signed update: a model-pack version or a
    /// downloader helper set. The verified bytes travel with the job; the
    /// chosen file is never read again.
    Update {
        signed: Arc<Vec<u8>>,
    },
    /// Select the previous version again; nothing is deleted.
    Rollback,
    /// Select the downloader baseline, recovering an unreadable state file.
    Baseline,
}

/// A verified signed update file waiting for the user's review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingUpdate {
    /// A pack identifier or [`DOWNLOADER`].
    pub target: String,
    /// One line naming what changes, in app terms.
    pub summary: String,
    /// The update's pack manifest, for its licenses and size.
    pub pack: Option<PackManifest>,
    /// License identifiers needing explicit acceptance before it installs.
    pub to_accept: Vec<String>,
    signed: Arc<Vec<u8>>,
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
    /// Running the updated downloader to confirm its versions.
    Probing,
    RollingBack,
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
            (Phase::Transferring, Work::Update { .. }) => "Downloading and verifying the update",
            (Phase::Transferring, _) => "Downloading and verifying",
            (Phase::SmokeTest, _) => "Testing the model on this Mac",
            (Phase::Activating, _) => "Activating",
            (Phase::Removing, _) => "Removing",
            (Phase::Discarding, _) => "Discarding the partial download",
            (Phase::Probing, _) => "Running the updated downloader on this Mac",
            (Phase::RollingBack, Work::Baseline) => "Switching to the baseline",
            (Phase::RollingBack, _) => "Rolling back",
        }
    }

    /// What kind of job this is, for "another job is running" messages.
    pub fn kind(&self) -> &'static str {
        match (&self.work, self.pack_id == DOWNLOADER) {
            (Work::Update { .. }, true) => "Downloader update",
            (Work::Update { .. }, false) => "Model update",
            (Work::Rollback | Work::Baseline, _) => "Rollback",
            (Work::Install { .. }, _) => "Model install",
            (Work::Remove | Work::RemoveVersion(_), _) => "Model removal",
            (Work::Discard, _) => "Discarding a partial download",
        }
    }

    pub fn fraction(&self) -> f32 {
        self.progress.completed_bytes as f32 / self.progress.total_bytes.max(1) as f32
    }

    pub fn installing(&self) -> bool {
        matches!(self.work, Work::Install { .. } | Work::Update { .. })
    }
}

/// How the last job for a pack ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ending {
    Installed,
    Removed,
    Discarded,
    /// A signed update was installed, tested and selected.
    Updated(String),
    /// The previous version was selected again.
    RolledBack(String),
    Cancelled,
    Failed(String),
}

/// The downloader helpers as the panel shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloaderView {
    pub summary: String,
    /// Why an installed update is not used, when it is not.
    pub note: Option<String>,
    /// A verification failure that makes imports refuse.
    pub problem: Option<String>,
    /// The selection a rollback would restore.
    pub previous: Option<String>,
    /// Whether switching to the baseline would change anything (an update
    /// is active, or the state is unreadable).
    pub baseline_available: bool,
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
    /// Each pack's previous version, kept for rollback.
    previous: Vec<Option<String>>,
    /// Why each pack's recorded active version is not in use, if it is not.
    notes: Vec<Option<String>>,
    free: Result<u64, String>,
    downloader: DownloaderView,
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
    /// Keys that verify signed updates: the compiled ones outside tests.
    keys: Vec<TrustedKey>,
    /// The managed downloader root where signed updates install.
    helpers_root: Result<PathBuf, String>,
    /// A verified update file the user is reviewing.
    pending: Option<PendingUpdate>,
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
            keys: trusted_keys(),
            helpers_root: deadpan_cli::youtube::helpers::default_root()
                .map_err(|error| error.to_string()),
            pending: None,
        }
    }

    /// Verify updates with these keys (replay signs with a test key).
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn set_trusted_keys(&mut self, keys: Vec<TrustedKey>) {
        self.keys = keys;
        self.snapshot = None;
    }

    /// Use a private downloader root.
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn set_helpers_root(&mut self, root: PathBuf) {
        self.helpers_root = Ok(root);
        self.snapshot = None;
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
        // Each family's selected version: an activated signed update, else
        // the compiled one.
        self.packs = approved_packs()
            .into_iter()
            .map(|approved| {
                store
                    .as_ref()
                    .and_then(|store| {
                        store
                            .selected_with_keys(&approved.pack_id, &self.keys)
                            .ok()
                            .flatten()
                    })
                    .unwrap_or(approved)
            })
            .collect();
        let previous = self
            .packs
            .iter()
            .map(|pack| {
                store
                    .as_ref()
                    .and_then(|store| store.pointer(&pack.pack_id).ok().flatten())
                    .and_then(|pointer| pointer.previous)
            })
            .collect();
        let notes = self
            .packs
            .iter()
            .map(|pack| {
                store.as_ref().and_then(|store| {
                    store
                        .selection_note_with_keys(&pack.pack_id, &self.keys)
                        .unwrap_or_else(|error| Some(error.to_string()))
                })
            })
            .collect();
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
            previous,
            notes,
            free,
            downloader: downloader_view(&self.helpers_root, &self.keys),
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

    /// The version a rollback of this pack would select again.
    pub fn previous(&self, pack_id: &str) -> Option<&str> {
        let index = self.packs.iter().position(|pack| pack.pack_id == pack_id)?;
        self.snapshot.as_ref()?.previous.get(index)?.as_deref()
    }

    pub fn note(&self, pack_id: &str) -> Option<&str> {
        let index = self.packs.iter().position(|pack| pack.pack_id == pack_id)?;
        self.snapshot.as_ref()?.notes.get(index)?.as_deref()
    }

    pub fn downloader(&self) -> Option<&DownloaderView> {
        self.snapshot.as_ref().map(|snapshot| &snapshot.downloader)
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
                "Another model job is running: {} ({}). Wait for it or cancel it first.",
                job.kind(),
                job.label().to_lowercase()
            ));
        }
        if pack_id == DOWNLOADER {
            if !matches!(work, Work::Rollback | Work::Baseline) {
                return Err("The downloader is updated from a signed update file.".into());
            }
            return self.spawn(pack_id, None, work, Vec::new(), 0, 0, repaint);
        }
        let manifest = self
            .pack(pack_id)
            .cloned()
            .ok_or_else(|| format!("{pack_id} is not an approved model pack."))?;
        let store = self.store().ok_or_else(|| self.storage_unavailable())?;
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
        self.spawn(
            pack_id,
            Some(manifest),
            work,
            accepted,
            completed,
            total,
            repaint,
        )
    }

    /// Verify a chosen signed update file and hold it for review: its
    /// signature, kind and target are checked here, so a refused file starts
    /// nothing, and nothing runs until [`Self::apply_pending`].
    pub fn review_update(&mut self, path: &Path) -> Result<(), String> {
        self.pending = None;
        let bytes = read_update(path)?;
        let envelope =
            SignedManifest::parse(&bytes).map_err(|error| update_refusal(&error.to_string()))?;
        let payload = envelope
            .verify_with(envelope.kind, &self.keys)
            .map_err(|error| update_refusal(&error.to_string()))?;
        let pending = match envelope.kind {
            UpdateKind::Downloader => {
                let manifest = downloader_updates::DownloaderManifest::parse(payload)
                    .map_err(|error| update_refusal(&error.to_string()))?;
                let helpers = manifest
                    .helpers
                    .iter()
                    .map(|release| format!("{} {}", release.name, release.version))
                    .collect::<Vec<_>>()
                    .join(" · ");
                PendingUpdate {
                    target: DOWNLOADER.into(),
                    summary: format!(
                        "YouTube downloader update {} ({}): {helpers}",
                        manifest.serial, manifest.issued
                    ),
                    pack: None,
                    to_accept: Vec::new(),
                    signed: Arc::new(bytes),
                }
            }
            UpdateKind::ModelPack => {
                let update = PackUpdate::parse(payload)
                    .map_err(|error| update_refusal(&error.to_string()))?;
                let pack = update.pack;
                PendingUpdate {
                    target: pack.pack_id.clone(),
                    summary: format!(
                        "{} version {} · {}",
                        pack.title,
                        pack.pack_version,
                        format_bytes(pack.total_bytes())
                    ),
                    to_accept: deadpan_models::packs::updates::licenses_to_accept(&pack),
                    pack: Some(pack),
                    signed: Arc::new(bytes),
                }
            }
        };
        self.pending = Some(pending);
        Ok(())
    }

    pub fn pending(&self) -> Option<&PendingUpdate> {
        self.pending.as_ref()
    }

    pub fn cancel_pending(&mut self) {
        self.pending = None;
    }

    /// Start the reviewed update. A pack update needs every license of
    /// `to_accept` in `accepted`; the store checks again before staging.
    pub fn apply_pending(
        &mut self,
        accepted: &BTreeMap<String, BTreeSet<String>>,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        if let Some(job) = self.job() {
            return Err(format!(
                "{} is running ({}). Wait for it or cancel it first.",
                job.kind(),
                job.label().to_lowercase()
            ));
        }
        let pending = self
            .pending
            .clone()
            .ok_or_else(|| "Choose a signed update file first.".to_owned())?;
        let accepted: Vec<String> = accepted
            .get(&pending.target)
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default();
        if let Some(missing) = pending.to_accept.iter().find(|id| !accepted.contains(id)) {
            let title = pending
                .pack
                .as_ref()
                .and_then(|pack| pack.licenses.iter().find(|license| &license.id == missing))
                .map_or(missing.clone(), |license| license.title.clone());
            return Err(format!("Accept the {title} to apply this update."));
        }
        let work = Work::Update {
            signed: Arc::clone(&pending.signed),
        };
        let total = match &pending.pack {
            Some(pack) => pack.total_bytes(),
            None => 0,
        };
        let target = pending.target.clone();
        self.spawn(&target, pending.pack, work, accepted, 0, total, repaint)?;
        self.pending = None;
        Ok(())
    }

    fn storage_unavailable(&self) -> String {
        format!(
            "Model storage is unavailable: {}",
            self.root.clone().err().unwrap_or_default()
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn(
        &mut self,
        target: &str,
        manifest: Option<PackManifest>,
        work: Work,
        accepted: Vec<String>,
        completed: u64,
        total: u64,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        let store = self.store().ok_or_else(|| self.storage_unavailable())?;
        let phase = match work {
            Work::Install { .. } | Work::Update { .. } => Phase::Transferring,
            Work::Remove | Work::RemoveVersion(_) => Phase::Removing,
            Work::Discard => Phase::Discarding,
            Work::Rollback | Work::Baseline => Phase::RollingBack,
        };
        let job = Job {
            pack_id: target.to_owned(),
            work: work.clone(),
            phase,
            progress: InstallProgress {
                completed_bytes: completed,
                total_bytes: total,
            },
            cancelling: false,
        };
        let context = Context {
            backend: self.backend.clone(),
            store,
            target: target.to_owned(),
            manifest,
            accepted,
            keys: self.keys.clone(),
            helpers_root: self.helpers_root.clone(),
        };
        let (sender, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let repaint = Arc::new(repaint);
        let thread = std::thread::Builder::new()
            .name("deadpan-model-pack".into())
            .spawn(move || {
                let send = |event: Event| {
                    let _ = sender.send(event);
                    repaint();
                };
                let ending = run(&context, &work, &thread_cancel, &send);
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

/// Everything a job thread needs, captured when it starts.
struct Context {
    backend: Backend,
    store: PackStore,
    /// A pack identifier or [`DOWNLOADER`].
    target: String,
    manifest: Option<PackManifest>,
    accepted: Vec<String>,
    keys: Vec<TrustedKey>,
    helpers_root: Result<PathBuf, String>,
}

fn failed(error: CliError, cancel: &AtomicBool) -> Ending {
    match error {
        _ if cancel.load(Ordering::Acquire) => Ending::Cancelled,
        CliError::ModelPack(PackError::Cancelled) => Ending::Cancelled,
        CliError::Import(error) if error.code == "DownloaderInstallCancelled" => Ending::Cancelled,
        CliError::ModelPack(error) => Ending::Failed(pack_failure(&error)),
        error => Ending::Failed(sentence(&error.to_string())),
    }
}

/// Install a pack version through the configured backend.
fn install_with(
    context: &Context,
    manifest: &PackManifest,
    source: Option<&Path>,
    cancel: &AtomicBool,
    send: &dyn Fn(Event),
) -> Result<(), CliError> {
    match &context.backend {
        Backend::Real => {
            let source = source.map(ImportSource::at).transpose()?;
            deadpan_cli::models::install_pack(
                &context.store,
                manifest,
                &context.accepted,
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
        Backend::Scripted(script) => script.install(
            &context.store,
            manifest,
            &context.accepted,
            cancel,
            &|event| send(event),
        ),
    }
}

fn run(context: &Context, work: &Work, cancel: &AtomicBool, send: &dyn Fn(Event)) -> Ending {
    let store = &context.store;
    if context.target == DOWNLOADER {
        return match work {
            Work::Update { signed } => update_downloader(context, signed, cancel, send),
            Work::Rollback => rollback_downloader(context, false),
            Work::Baseline => rollback_downloader(context, true),
            _ => Ending::Failed("The downloader supports only update and rollback.".into()),
        };
    }
    let Some(manifest) = &context.manifest else {
        return Ending::Failed("No model pack was named.".into());
    };
    match work {
        Work::Install { source } => {
            match install_with(context, manifest, source.as_deref(), cancel, send) {
                Ok(()) => Ending::Installed,
                Err(error) => failed(error, cancel),
            }
        }
        Work::Remove => match store.remove(manifest) {
            Ok(()) => Ending::Removed,
            Err(error) => Ending::Failed(sentence(&error.to_string())),
        },
        Work::RemoveVersion(version) => {
            let other = store
                .catalog_with_keys(&context.keys)
                .into_iter()
                .find(|known| known.pack_id == manifest.pack_id && &known.pack_version == version);
            match other.map(|other| store.remove(&other)) {
                Some(Ok(())) => Ending::Removed,
                Some(Err(error)) => Ending::Failed(sentence(&error.to_string())),
                None => Ending::Failed(format!(
                    "Version {version} is not a known version of this pack."
                )),
            }
        }
        Work::Baseline => Ending::Failed("Packs roll back to a version, not a baseline.".into()),
        Work::Discard => match store.discard_partial(manifest) {
            Ok(()) => Ending::Discarded,
            Err(error) => Ending::Failed(pack_failure(&error)),
        },
        Work::Update { signed } => {
            let result = (|| -> Result<String, CliError> {
                let admitted =
                    store.admit_update(signed, &context.keys, &context.accepted, false)?;
                if store.installed(&admitted)?.is_none() {
                    install_with(context, &admitted, None, cancel, send)?;
                }
                // Retained and selected only after its smoke test passed.
                store.activate_update(signed, &context.keys, &context.accepted, false)?;
                Ok(admitted.pack_version)
            })();
            match result {
                Ok(version) => Ending::Updated(format!("version {version}")),
                Err(error) => failed(error, cancel),
            }
        }
        Work::Rollback => match store.rollback_with_keys(&manifest.pack_id, &context.keys) {
            Ok(installed) => {
                Ending::RolledBack(format!("version {}", installed.manifest.pack_version))
            }
            Err(error) => Ending::Failed(pack_failure(&error)),
        },
    }
}

fn update_downloader(
    context: &Context,
    bytes: &[u8],
    cancel: &AtomicBool,
    send: &dyn Fn(Event),
) -> Ending {
    if !matches!(context.backend, Backend::Real) {
        return Ending::Failed("Replay never runs the downloader.".into());
    }
    let root = match &context.helpers_root {
        Ok(root) => root,
        Err(error) => return Ending::Failed(sentence(error)),
    };
    use deadpan_cli::youtube::helpers::USER_AGENT;
    let transport = deadpan_models::packs::HttpsTransport::with_user_agent(USER_AGENT);
    let mut base = 0u64;
    let mut current = 0u64;
    let mut total = 0u64;
    let result = downloader_updates::update(
        root,
        bytes,
        downloader_updates::UpdateOptions {
            keys: &context.keys,
            allow_downgrade: false,
        },
        &transport,
        cancel,
        &deadpan_cli::youtube::acquire::probe,
        &mut |event| match event["event"].as_str() {
            Some("installing") => {
                base += current;
                current = event["bytes"].as_u64().unwrap_or(0);
                total = total.max(base + current);
            }
            Some("progress") => send(Event::Progress(InstallProgress {
                completed_bytes: base + event["completed_bytes"].as_u64().unwrap_or(0),
                total_bytes: total.max(1),
            })),
            Some("probing") => send(Event::Phase(Phase::Probing)),
            _ => {}
        },
    );
    match result {
        Ok(outcome) => Ending::Updated(format!("signed update {}", outcome.serial)),
        Err(error) => failed(error, cancel),
    }
}

fn rollback_downloader(context: &Context, to_baseline: bool) -> Ending {
    let root = match &context.helpers_root {
        Ok(root) => root,
        Err(error) => return Ending::Failed(sentence(error)),
    };
    let result = HelperSource::default_baseline().and_then(|baseline| {
        downloader_updates::rollback(root, &baseline, &context.keys, to_baseline)
    });
    match result {
        Ok(outcome) => Ending::RolledBack(match outcome.recovered {
            Some(problem) => format!(
                "{} (replaced an unreadable state: {problem})",
                selected_label(outcome.state.active)
            ),
            None => selected_label(outcome.state.active),
        }),
        Err(error) => Ending::Failed(sentence(&error.to_string())),
    }
}

fn selected_label(selected: downloader_updates::Selected) -> String {
    match selected {
        downloader_updates::Selected::Baseline => "the baseline".into(),
        downloader_updates::Selected::Update { serial } => format!("signed update {serial}"),
    }
}

/// The downloader's selected helpers, read without hashing.
fn downloader_view(root: &Result<PathBuf, String>, keys: &[TrustedKey]) -> DownloaderView {
    let unavailable = |problem: String| DownloaderView {
        summary: "Downloader state is unavailable.".into(),
        note: None,
        problem: Some(problem),
        previous: None,
        baseline_available: false,
    };
    let root = match root {
        Ok(root) => root,
        Err(error) => return unavailable(error.clone()),
    };
    let baseline = match HelperSource::default_baseline() {
        Ok(baseline) => baseline,
        Err(error) => return unavailable(error.to_string()),
    };
    let state = downloader_updates::state(root);
    let previous = state
        .as_ref()
        .ok()
        .and_then(Option::as_ref)
        .and_then(|state| state.previous)
        .map(selected_label);
    // An active update, or an unreadable state file, can always be left for
    // the baseline (which also rewrites the state).
    let off_baseline = match &state {
        Ok(state) => state
            .as_ref()
            .is_some_and(|state| state.active != downloader_updates::Selected::Baseline),
        Err(_) => true,
    };
    match downloader_updates::select_with_keys(root, baseline, keys) {
        Ok(selection) => {
            let version = |pin| {
                selection
                    .source
                    .release(pin)
                    .map(|release| release.version)
                    .unwrap_or_default()
            };
            let origin = match &selection.source {
                HelperSource::Bundled(_) => "bundled with Deadpan".to_owned(),
                HelperSource::Managed(_) => "pinned baseline".to_owned(),
                HelperSource::Update(update) => format!("signed update {}", update.serial),
            };
            DownloaderView {
                summary: format!(
                    "yt-dlp {} · Deno {} · {origin}",
                    version(&YT_DLP),
                    version(&DENO)
                ),
                note: selection.note,
                problem: None,
                previous,
                baseline_available: off_baseline,
            }
        }
        Err(error) => DownloaderView {
            previous,
            baseline_available: true,
            ..unavailable(sentence(&error.to_string()))
        },
    }
}

/// A bounded read of a chosen signed update file.
fn read_update(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let limit = deadpan_models::updates::MAX_SIGNED_BYTES;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("The update file could not be read: {error}."))?;
    if bytes.len() as u64 > limit {
        return Err("The update file is larger than 256 KiB, so it is not a signed update.".into());
    }
    Ok(bytes)
}

/// A refused update file in app terms.
fn update_refusal(reason: &str) -> String {
    format!("This update was not applied: {}", sentence(reason))
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

    /// A newer version of an approved pack signed with a fresh test key,
    /// written to `directory`. Returns the file and the key that verifies it.
    pub fn signed_pack_update(
        directory: &Path,
        pack_id: &str,
        version: &str,
        key_id: &str,
    ) -> std::io::Result<(PathBuf, TrustedKey)> {
        let invalid = |error: String| std::io::Error::other(error);
        let (pkcs8, public_key) =
            deadpan_models::updates::generate_key().map_err(|error| invalid(error.to_string()))?;
        let mut pack = deadpan_models::packs::approved_pack(pack_id)
            .ok_or_else(|| invalid(format!("no approved pack {pack_id}")))?;
        pack.pack_version = version.into();
        let update = PackUpdate {
            schema: deadpan_models::packs::updates::UPDATE_SCHEMA,
            serial: 1,
            issued: "2026-10-06".into(),
            min_app_version: "0.1.0".into(),
            pack,
        };
        let signed = SignedManifest::sign(
            UpdateKind::ModelPack,
            serde_json::to_string(&update).map_err(|error| invalid(error.to_string()))?,
            key_id,
            &pkcs8,
        )
        .map_err(|error| invalid(error.to_string()))?;
        std::fs::create_dir_all(directory)?;
        let path = directory.join(format!("{pack_id}-{version}.signed.json"));
        std::fs::write(&path, signed.to_bytes())?;
        Ok((
            path,
            TrustedKey {
                id: key_id.into(),
                public_key,
            },
        ))
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
    fn signed_pack_updates_install_select_and_roll_back() {
        let root = tempfile::tempdir().unwrap();
        let complete = || Run {
            steps: 2,
            interval: Duration::from_millis(1),
            finish: Finish::Complete,
        };
        let script = Script::new([complete(), complete()]);
        let mut manager = Manager::new(Backend::Scripted(Arc::new(script)));
        manager.set_root(root.path().join("models"));
        manager.set_helpers_root(root.path().join("helpers"));
        let (path, key) =
            super::scripted::signed_pack_update(root.path(), WHISPER, "3", "test-key").unwrap();
        let (untrusted, _) = super::scripted::signed_pack_update(
            &root.path().join("other"),
            WHISPER,
            "4",
            "other-key",
        )
        .unwrap();
        manager.set_trusted_keys(vec![key]);
        manager.refresh();
        let none = BTreeMap::new();
        let refused = manager.review_update(&untrusted).unwrap_err();
        assert!(manager.pending().is_none());
        assert!(
            refused.contains("not applied") && refused.contains("unknown signing key"),
            "{refused}"
        );
        assert!(manager.job().is_none());

        manager
            .start(
                WHISPER,
                Work::Install { source: None },
                &accepted(&[]),
                || {},
            )
            .unwrap();
        wait(&mut manager);
        assert_eq!(manager.pack(WHISPER).unwrap().pack_version, "2");
        // Reviewing holds the verified update; nothing runs yet.
        manager.review_update(&path).unwrap();
        let pending = manager.pending().unwrap();
        assert_eq!(pending.target, WHISPER);
        assert!(pending.to_accept.is_empty());
        assert!(manager.job().is_none());
        // The chosen file may change or vanish; the verified bytes travel.
        std::fs::remove_file(&path).unwrap();
        manager.apply_pending(&none, || {}).unwrap();
        assert!(manager.pending().is_none());
        assert_eq!(manager.job().unwrap().pack_id, WHISPER);
        wait(&mut manager);
        assert_eq!(
            manager.outcome().unwrap().ending,
            Ending::Updated("version 3".into())
        );
        assert_eq!(manager.pack(WHISPER).unwrap().pack_version, "3");
        assert_eq!(manager.previous(WHISPER), Some("2"));
        assert!(manager.installed(WHISPER));

        manager
            .start(WHISPER, Work::Rollback, &accepted(&[]), || {})
            .unwrap();
        wait(&mut manager);
        assert_eq!(
            manager.outcome().unwrap().ending,
            Ending::RolledBack("version 2".into())
        );
        assert_eq!(manager.pack(WHISPER).unwrap().pack_version, "2");
        assert_eq!(manager.previous(WHISPER), Some("3"));
        // Both versions stay installed.
        assert!(
            root.path()
                .join("models/whisper-base-en/2/receipt.json")
                .is_file()
        );
        assert!(
            root.path()
                .join("models/whisper-base-en/3/receipt.json")
                .is_file()
        );

        // Outside a bundle the downloader baseline is the compiled pins.
        let view = manager.downloader().unwrap();
        assert!(view.summary.contains("yt-dlp 2026.08.19"), "{view:?}");
        assert!(view.summary.contains("pinned baseline"), "{view:?}");
        assert_eq!(view.previous, None);
        assert!(
            manager
                .start(DOWNLOADER, Work::Rollback, &accepted(&[]), || {})
                .is_ok()
        );
        wait(&mut manager);
        assert!(matches!(
            manager.outcome().unwrap().ending,
            Ending::Failed(_)
        ));
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
