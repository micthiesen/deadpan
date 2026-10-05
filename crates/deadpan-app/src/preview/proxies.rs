//! Preview proxy of the Original, built in the background.
//!
//! Once a single-Original project's Original is ready (including a project
//! that opens without a proxy), one cancellable job thread at utility
//! priority checks the per-user proxy cache, removes stale entries and, when
//! the Original needs a proxy, runs the isolated media worker, verifies its
//! output against the Original and publishes it. Nothing waits for it: the
//! viewer decodes the Original until a verified proxy appears, and every
//! edit, export and render ignores proxies entirely.
//!
//! The job respects the machine: `:proxies off` disables automatic proxies
//! (remembered per user), and the worker is suspended while the edit plays,
//! while a render runs, on battery, in Low Power Mode or under thermal
//! pressure. Free space and the cache budget are checked first, and a
//! failed build is remembered for that Original and recipe until
//! `:proxies retry`. The Original card says what the job is doing and why.

use std::path::PathBuf;
use std::sync::mpsc;

use deadpan_cli::proxy::cache::{ProxyCache, ProxyCleanupPolicy};
use deadpan_cli::proxy::{
    BuildControl, ProxyStatus, build_proxy, media_worker, power_guard, proxy_key, proxy_status,
};
use deadpan_store::original_media::OriginalMediaLimits;

use super::*;

/// How often the job rereads power and thermal state while it runs.
const POWER_POLL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) enum ProxyState {
    /// Not yet checked for this project session.
    #[default]
    Unchecked,
    Building,
    /// The build is suspended; the reason says why.
    Paused(String),
    /// A verified proxy is published.
    Ready,
    /// The Original seeks fast enough without one.
    NotNeeded,
    /// A proxy cannot represent this Original exactly.
    Ineligible(String),
    /// `:proxies off`.
    Disabled,
    /// No cache or no media worker on this machine.
    Unavailable(String),
    /// This build, or a remembered earlier one, failed.
    Failed(String),
}

enum Event {
    State(ProxyState),
    Done(ProxyState),
}

use crate::navigation::command::ProxyCommand;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    automatic: bool,
}

pub(super) struct ProxyJob {
    session: Option<u64>,
    pub(super) state: ProxyState,
    events: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    /// Set by the UI while the edit plays or a render runs.
    busy: Arc<AtomicBool>,
    /// The running build and cancelled builds of earlier sessions.
    threads: Vec<std::thread::JoinHandle<()>>,
    pub(super) cache: Option<ProxyCache>,
    cache_error: Option<String>,
    settings: Option<PathBuf>,
    automatic: bool,
}

impl Default for ProxyJob {
    fn default() -> Self {
        let (cache, cache_error) = match deadpan_cli::proxy::cache::default_root() {
            Some(root) => match ProxyCache::at(&root) {
                Ok(cache) => (Some(cache), None),
                Err(error) => (None, Some(error.to_string())),
            },
            None => (None, Some("no home directory".into())),
        };
        let settings = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
            .map(|home| home.join("Library/Application Support/Deadpan/proxies.json"));
        let mut job = Self {
            session: None,
            state: ProxyState::Unchecked,
            events: None,
            cancel: Arc::default(),
            busy: Arc::default(),
            threads: Vec::new(),
            cache,
            cache_error,
            settings: None,
            automatic: true,
        };
        job.use_settings(settings);
        job
    }
}

impl ProxyJob {
    /// Read the per-user setting from `path` (absent means automatic).
    pub(super) fn use_settings(&mut self, path: Option<PathBuf>) {
        self.automatic = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
            .is_none_or(|settings| settings.automatic);
        self.settings = path;
    }

    /// Use another cache root, such as a replay's private one.
    #[cfg(feature = "ui-harness")]
    pub(super) fn use_cache(&mut self, cache: Option<ProxyCache>) {
        self.cache = cache;
        self.cache_error = None;
        self.reset(self.session);
    }

    fn save_settings(&self) -> Result<(), String> {
        let path = self.settings.as_ref().ok_or("no settings location")?;
        let directory = path.parent().ok_or("invalid settings location")?;
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec(&Settings {
            automatic: self.automatic,
        })
        .map_err(|error| error.to_string())?;
        let temporary = directory.join(format!(".proxies-{}.json", uuid::Uuid::new_v4().simple()));
        std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, path).map_err(|error| error.to_string())
    }

    /// A new project session cancels the previous session's build.
    fn reset(&mut self, session: Option<u64>) {
        self.cancel.store(true, Ordering::Release);
        self.threads.retain(|thread| !thread.is_finished());
        self.session = session;
        self.state = ProxyState::Unchecked;
        self.events = None;
        self.cancel = Arc::default();
    }

    /// Cancel and wait briefly: cancellation stops the worker process, and
    /// no private snapshot outlives the app.
    pub(super) fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::Release);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        for thread in std::mem::take(&mut self.threads) {
            while !thread.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }

    /// Replay-visible state name.
    #[cfg(feature = "ui-harness")]
    pub(super) fn state_name(&self) -> &'static str {
        match self.state {
            ProxyState::Unchecked => "unchecked",
            ProxyState::Building => "building",
            ProxyState::Paused(_) => "paused",
            ProxyState::Ready => "ready",
            ProxyState::NotNeeded => "not_needed",
            ProxyState::Ineligible(_) => "ineligible",
            ProxyState::Disabled => "disabled",
            ProxyState::Unavailable(_) => "unavailable",
            ProxyState::Failed(_) => "failed",
        }
    }

    /// The Original card's suffix, when the proxy state matters to the user.
    pub(super) fn detail(&self) -> Option<&'static str> {
        match self.state {
            ProxyState::Building => Some("preparing seek proxy"),
            ProxyState::Paused(_) => Some("seek proxy paused"),
            ProxyState::Failed(_) => Some("seek proxy failed"),
            ProxyState::Ineligible(_) => Some("no seek proxy"),
            _ => None,
        }
    }

    /// The Original card's hover explanation.
    pub(super) fn explanation(&self) -> Option<String> {
        match &self.state {
            ProxyState::Building => Some(
                "A smaller copy of the Original is being prepared so seeking in this 4K video is fast. Exact pictures and export always use the Original.".into(),
            ),
            ProxyState::Paused(reason) => Some(format!(
                "Preparing the seek proxy is paused because {reason}. It continues when that ends."
            )),
            ProxyState::Failed(error) => Some(format!(
                "Seeking uses the Original because its proxy could not be made: {error}. Use :proxies retry to try again."
            )),
            ProxyState::Ineligible(reason) => Some(format!(
                "Seeking uses the Original: {reason}."
            )),
            _ => None,
        }
    }
}

fn ineligible_reason(reason: &deadpan_media::proxy::ProxyIneligible) -> String {
    use deadpan_media::proxy::ProxyIneligible;
    match reason {
        ProxyIneligible::IrregularDurations => "some of its pictures last longer or shorter than the gap to the next picture, which a proxy file cannot reproduce exactly".into(),
        ProxyIneligible::MissingDuration => "its last picture has no measured duration".into(),
        ProxyIneligible::Empty => "it has no pictures".into(),
    }
}

impl DeadpanApp {
    /// `:proxies on|off|retry`.
    pub(super) fn proxy_command(&mut self, command: ProxyCommand) {
        match command {
            ProxyCommand::On | ProxyCommand::Off => {
                self.proxies.automatic = command == ProxyCommand::On;
                let saved = self.proxies.save_settings();
                self.proxies.reset(self.proxies.session);
                self.message = Some(match (command, saved) {
                    (ProxyCommand::On, Ok(())) => "Automatic seek proxies are on.".into(),
                    (_, Ok(())) => {
                        "Automatic seek proxies are off. Seeking uses the Original.".into()
                    }
                    (_, Err(error)) => {
                        format!("Seek proxies changed for this session only: {error}")
                    }
                });
            }
            ProxyCommand::Retry => {
                let forgotten = self.proxy_key().and_then(|key| {
                    let cache = self.proxies.cache.as_ref()?;
                    Some(
                        cache
                            .forget_failure(&key)
                            .map_err(|error| error.to_string()),
                    )
                });
                self.proxies.reset(self.proxies.session);
                self.message = Some(match forgotten {
                    Some(Err(error)) => format!("Could not clear the proxy failure: {error}"),
                    _ => "Trying the seek proxy again.".into(),
                });
            }
        }
    }

    fn proxy_key(&self) -> Option<deadpan_cli::proxy::cache::ProxyKey> {
        let workspace = self.workspace.as_ref()?;
        let Some(SingleSourceState::Ready { asset, .. }) = &workspace.single_source else {
            return None;
        };
        let registered = workspace.sources.get(asset)?;
        proxy_key(&registered.original, registered.receipt.snapshot().video()?).ok()
    }

    /// Advance the proxy build once per outer frame.
    pub(super) fn reconcile_proxies(&mut self, context: &egui::Context) {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        if self.proxies.session != session {
            self.proxies.reset(session);
        }
        // Playback and renders take priority: suspend the worker meanwhile.
        self.proxies.busy.store(
            self.transport.is_some() || self.render.blocking(),
            Ordering::Release,
        );
        while let Some(event) = self
            .proxies
            .events
            .as_ref()
            .and_then(|events| events.try_recv().ok())
        {
            match event {
                Event::State(state) => self.proxies.state = state,
                Event::Done(state) => {
                    self.proxies.events = None;
                    self.proxies.state = state;
                }
            }
            context.request_repaint();
        }
        if self.proxies.state != ProxyState::Unchecked || self.proxies.events.is_some() {
            return;
        }
        if !self.proxies.automatic {
            self.proxies.state = ProxyState::Disabled;
            return;
        }
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let Some(SingleSourceState::Ready { asset, .. }) = &workspace.single_source else {
            return;
        };
        let Some(registered) = workspace.sources.get(asset).cloned() else {
            return;
        };
        let Some(cache) = self.proxies.cache.clone() else {
            self.proxies.state = ProxyState::Unavailable(
                self.proxies
                    .cache_error
                    .clone()
                    .unwrap_or_else(|| "no proxy cache".into()),
            );
            return;
        };
        let worker = media_worker();
        let originals = workspace.originals.clone();
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.proxies.cancel = Arc::clone(&cancel);
        self.proxies.events = Some(receiver);
        self.proxies.state = ProxyState::Building;
        let busy = Arc::clone(&self.proxies.busy);
        let repaint = context.clone();
        let spawned = std::thread::Builder::new()
            .name("deadpan-proxy".into())
            .spawn(move || {
                deadpan_source::lower_current_thread_priority();
                let send = |event| {
                    let _ = sender.send(event);
                    repaint.request_repaint();
                };
                let state = build(
                    &cache,
                    &registered,
                    &originals,
                    worker.as_deref(),
                    &cancel,
                    &busy,
                    &send,
                );
                send(Event::Done(state));
            });
        match spawned {
            Ok(thread) => self.proxies.threads.push(thread),
            Err(error) => {
                self.proxies.events = None;
                self.proxies.state = ProxyState::Failed(error.to_string());
            }
        }
    }
}

/// The job thread: check, then build with a monitor that suspends the
/// worker while the app is busy or the machine should rest.
fn build(
    cache: &ProxyCache,
    registered: &Arc<crate::project::RegisteredSource>,
    originals: &deadpan_store::original_media::OriginalImportHandle,
    worker: Option<&std::path::Path>,
    cancel: &AtomicBool,
    busy: &AtomicBool,
    send: &(dyn Fn(Event) + Sync),
) -> ProxyState {
    let Some(video) = registered.receipt.snapshot().video() else {
        return ProxyState::NotNeeded;
    };
    let Ok(key) = proxy_key(&registered.original, video) else {
        return ProxyState::NotNeeded;
    };
    // Abandoned staging, stale recipes and entries unused for a month.
    let _ = cache.cleanup(std::slice::from_ref(&key), ProxyCleanupPolicy::default());
    match proxy_status(cache, &registered.original, video) {
        Ok(ProxyStatus::NotNeeded) => return ProxyState::NotNeeded,
        Ok(ProxyStatus::Ineligible(reason)) => {
            return ProxyState::Ineligible(ineligible_reason(&reason));
        }
        Ok(ProxyStatus::Failed(message)) => return ProxyState::Failed(message),
        // Checked once per file state; no worker needed when intact.
        Ok(ProxyStatus::Ready(entry)) if cache.open(&entry, cancel).is_ok() => {
            return ProxyState::Ready;
        }
        Ok(ProxyStatus::Ready(_) | ProxyStatus::Missing(_)) => {}
        Err(error) => return ProxyState::Failed(error.to_string()),
    }
    let Some(worker) = worker else {
        return ProxyState::Unavailable("the media worker is not installed beside the app".into());
    };
    let pause = AtomicBool::new(false);
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut power: Option<(std::time::Instant, Option<String>)> = None;
            let mut reported: Option<Option<String>> = None;
            while !done.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire) {
                if power
                    .as_ref()
                    .is_none_or(|(checked, _)| checked.elapsed() >= POWER_POLL)
                {
                    power = Some((std::time::Instant::now(), power_guard()));
                }
                let reason = if busy.load(Ordering::Acquire) {
                    Some("the edit is playing or rendering".to_owned())
                } else {
                    power.as_ref().and_then(|(_, reason)| reason.clone())
                };
                pause.store(reason.is_some(), Ordering::Release);
                if reported.as_ref() != Some(&reason) {
                    send(Event::State(match &reason {
                        Some(reason) => ProxyState::Paused(reason.clone()),
                        None => ProxyState::Building,
                    }));
                    reported = Some(reason);
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        });
        let original = registered.original.clone();
        let result = build_proxy(
            cache,
            &registered.original,
            video,
            worker,
            Box::new(move |cancelled| {
                let snapshot = originals.snapshot_original(
                    &original,
                    OriginalMediaLimits::new(
                        original.object().byte_length().max(1),
                        Duration::from_secs(3600),
                    )
                    .map_err(deadpan_store::StoreError::from)?,
                    cancelled,
                )?;
                Ok(Box::new(snapshot) as Box<dyn std::io::Read>)
            }),
            BuildControl {
                pause: Some(&pause),
                ..BuildControl::new(cancel)
            },
        );
        done.store(true, Ordering::Release);
        match result {
            Ok(ProxyStatus::Ready(_)) => ProxyState::Ready,
            Ok(ProxyStatus::Failed(message)) => ProxyState::Failed(message),
            Ok(ProxyStatus::Ineligible(reason)) => {
                ProxyState::Ineligible(ineligible_reason(&reason))
            }
            Ok(_) => ProxyState::NotNeeded,
            Err(error) if error.is_cancellation() => ProxyState::Unchecked,
            Err(error) => ProxyState::Failed(error.to_string()),
        }
    })
}
