//! One native dialog, polled by the live application's event loop.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    /// Choose the original video; the project package location is automatic.
    CreateProject,
    CreateLinkedProject,
    InitializeSource,
    InitializeLinkedSource,
    OpenProject,
    ImportSound,
    /// Generic/legacy host media registration.
    ImportMedia,
    Render,
    /// An explicit Netscape cookies file for one YouTube import.
    Cookies,
    /// The moved or restored file of a missing Original.
    RelinkOriginal,
    /// A folder holding a model pack's files, for an offline install.
    ModelPackFolder,
    /// An uncompressed tar archive of a model pack, for an offline install.
    ModelPackArchive,
    /// A signed model-pack or downloader update manifest (.json).
    SignedUpdate,
    /// Where File › Save Portable Copy… writes a new self-contained package.
    PortableCopy,
    /// An explicit local diagnostic report with no user content attached.
    DiagnosticReport,
}

pub struct SaveMovie {
    pub directory: PathBuf,
    pub name: String,
}

pub struct DialogResult {
    pub kind: DialogKind,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}

type DialogFuture = Pin<Box<dyn Future<Output = Option<PathBuf>> + Send>>;

struct PendingDialog {
    kind: DialogKind,
    future: DialogFuture,
    waker: Waker,
    directory: Option<std::sync::mpsc::Receiver<Result<SaveMovie, String>>>,
}

struct Repaint(egui::Context);

impl Wake for Repaint {
    fn wake(self: Arc<Self>) {
        self.0.request_repaint();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.request_repaint();
    }
}

#[derive(Default)]
pub struct Dialogs {
    pending: Option<PendingDialog>,
    #[cfg(feature = "ui-harness")]
    scripted: Option<std::collections::VecDeque<(DialogKind, Option<PathBuf>)>>,
}

impl Dialogs {
    /// Call only from the main thread during an update of the live window.
    /// Native sheet creation happens here; subsequent polls only inspect its result.
    pub fn start(&mut self, kind: DialogKind, context: &egui::Context) -> Result<(), String> {
        self.start_with_save(kind, None, context)
    }

    pub fn save_movie(&mut self, save: SaveMovie, context: &egui::Context) -> Result<(), String> {
        self.start_with_save(DialogKind::Render, Some(save), context)
    }

    fn start_with_save(
        &mut self,
        kind: DialogKind,
        save: Option<SaveMovie>,
        context: &egui::Context,
    ) -> Result<(), String> {
        if self.is_open() {
            return Err("Finish or cancel the open dialog before opening another.".into());
        }
        let waker = Waker::from(Arc::new(Repaint(context.clone())));
        let mut directory = None;
        #[cfg(feature = "ui-harness")]
        let future: DialogFuture = if let Some(scripted) = &mut self.scripted {
            let (expected, path) = scripted
                .pop_front()
                .ok_or("No scripted dialog response; native dialogs are disabled in UI replay")?;
            if expected != kind {
                return Err("Unexpected dialog kind in UI replay".into());
            }
            Box::pin(std::future::ready(path))
        } else {
            prepare_dialog(kind, save, &waker, &mut directory)?
        };
        #[cfg(not(feature = "ui-harness"))]
        let future = prepare_dialog(kind, save, &waker, &mut directory)?;
        self.pending = Some(PendingDialog {
            kind,
            future,
            waker,
            directory,
        });
        // Register the completion waker on the next update, even if the UI is idle.
        context.request_repaint();
        Ok(())
    }

    pub fn take_result(&mut self) -> Option<DialogResult> {
        let pending = self.pending.as_mut()?;
        if let Some(directory) = &pending.directory {
            use std::sync::mpsc::TryRecvError;
            let prepared = match directory.try_recv() {
                Ok(value) => value,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    Err("Render destination preparation stopped.".into())
                }
            };
            match prepared.and_then(|save| native_dialog(pending.kind, Some(save))) {
                Ok(future) => {
                    pending.future = future;
                    pending.directory = None;
                }
                Err(error) => {
                    let kind = pending.kind;
                    self.pending = None;
                    return Some(DialogResult {
                        kind,
                        path: None,
                        error: Some(error),
                    });
                }
            }
        }
        let mut context = Context::from_waker(&pending.waker);
        let Poll::Ready(path) = pending.future.as_mut().poll(&mut context) else {
            return None;
        };
        let kind = pending.kind;
        self.pending = None;
        Some(DialogResult {
            kind,
            path,
            error: None,
        })
    }

    pub fn is_open(&self) -> bool {
        self.pending.is_some()
    }

    /// Replace only the operating-system picker result. Application dialog
    /// intent, input ownership and import preparation still run normally.
    #[cfg(feature = "ui-harness")]
    pub(crate) fn scripted(responses: Vec<(DialogKind, Option<PathBuf>)>) -> Self {
        Self {
            pending: None,
            scripted: Some(responses.into()),
        }
    }
}

/// Directory creation is filesystem work. Keep it off the native event thread;
/// construct the actual save sheet only when the main-thread poll admits it.
fn prepare_dialog(
    kind: DialogKind,
    save: Option<SaveMovie>,
    waker: &Waker,
    directory: &mut Option<std::sync::mpsc::Receiver<Result<SaveMovie, String>>>,
) -> Result<DialogFuture, String> {
    let Some(save) = save else {
        return native_dialog(kind, None);
    };
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let waker = waker.clone();
    std::thread::Builder::new()
        .name("deadpan-export-directory".into())
        .spawn(move || {
            let result = match std::fs::create_dir(&save.directory) {
                Ok(()) => Ok(save),
                Err(error)
                    if error.kind() == std::io::ErrorKind::AlreadyExists
                        && save.directory.is_dir() =>
                {
                    Ok(save)
                }
                Err(error) => Err(format!("Cannot prepare the export directory: {error}")),
            };
            let _ = sender.send(result);
            waker.wake();
        })
        .map_err(|error| error.to_string())?;
    *directory = Some(receiver);
    Ok(Box::pin(std::future::pending()))
}

#[cfg(target_os = "macos")]
fn native_dialog(kind: DialogKind, save: Option<SaveMovie>) -> Result<DialogFuture, String> {
    // Construct on the main thread so rfd attaches an asynchronous sheet to the
    // running NSApplication. Moving construction into an async block would defer
    // that requirement to whoever first polls it.
    let future: Pin<Box<dyn Future<Output = Option<rfd::FileHandle>> + Send>> = match kind {
        DialogKind::Render => {
            let save = save.ok_or("A Render destination suggestion is required.")?;
            Box::pin(
                rfd::AsyncFileDialog::new()
                    .set_title("Render your edit")
                    .set_directory(save.directory)
                    .set_file_name(save.name)
                    .add_filter("MP4 movie", &["mp4"])
                    .save_file(),
            )
        }
        DialogKind::CreateProject | DialogKind::InitializeSource => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Choose the Original video")
                .add_filter("Qualified video containers", &["mp4", "m4v", "mkv", "webm"])
                .pick_file(),
        ),
        DialogKind::CreateLinkedProject | DialogKind::InitializeLinkedSource => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Link the Original video at its current location")
                .add_filter("Qualified video containers", &["mp4", "m4v", "mkv", "webm"])
                .pick_file(),
        ),
        DialogKind::OpenProject => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Open a Deadpan project (.deadpan)")
                .set_directory(crate::library::ProjectLibrary::documents()?.root())
                .set_can_create_directories(false)
                .pick_folder(),
        ),
        DialogKind::Cookies => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Choose a cookies file (Netscape format) for this import")
                .add_filter("Cookies file", &["txt"])
                .pick_file(),
        ),
        DialogKind::RelinkOriginal => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Locate the missing Original (its content must be identical)")
                .pick_file(),
        ),
        DialogKind::ModelPackFolder => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Choose a folder holding the model pack's files")
                .set_can_create_directories(false)
                .pick_folder(),
        ),
        DialogKind::PortableCopy => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Save a portable copy of this project")
                .set_file_name("Portable copy.deadpan")
                .add_filter("Deadpan project", &["deadpan"])
                .save_file(),
        ),
        DialogKind::DiagnosticReport => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Save a private diagnostic report")
                .set_file_name("Deadpan diagnostics.json")
                .add_filter("Diagnostic report", &["json"])
                .save_file(),
        ),
        DialogKind::ModelPackArchive => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Choose a model pack archive (.tar)")
                .add_filter("Uncompressed tar archive", &["tar"])
                .pick_file(),
        ),
        DialogKind::SignedUpdate => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Choose a signed Deadpan update (.json)")
                .add_filter("Signed update manifest", &["json"])
                .pick_file(),
        ),
        DialogKind::ImportSound => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Add a sound (audio stream only)")
                .add_filter(
                    "Qualified audio containers",
                    &["wav", "mp4", "m4a", "m4v", "webm", "mkv", "mka", "mp3"],
                )
                .pick_file(),
        ),
        DialogKind::ImportMedia => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Import media")
                .add_filter(
                    "Video and audio",
                    &[
                        "mp4", "m4v", "mov", "mkv", "webm", "avi", "wav", "aif", "aiff", "flac",
                        "mp3", "m4a", "ogg",
                    ],
                )
                .pick_file(),
        ),
    };
    Ok(Box::pin(async move {
        future.await.map(|file| file.path().to_path_buf())
    }))
}

#[cfg(not(target_os = "macos"))]
fn native_dialog(_kind: DialogKind, _save: Option<SaveMovie>) -> Result<DialogFuture, String> {
    Err("Native file dialogs are supported only on macOS.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_directory_preparation_creates_only_the_neighbor_folder_and_preserves_collisions() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Exports");
        let waker = Waker::from(Arc::new(Repaint(egui::Context::default())));
        let mut directory = None;
        let _pending = prepare_dialog(
            DialogKind::Render,
            Some(SaveMovie {
                directory: folder.clone(),
                name: "edit.mp4".into(),
            }),
            &waker,
            &mut directory,
        )
        .unwrap();
        let prepared = directory
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(prepared.directory, folder);
        assert!(folder.is_dir());
        assert!(!folder.join("edit.mp4").exists());

        let collision = root.path().join("existing-movie.mp4");
        std::fs::write(&collision, b"keep this movie").unwrap();
        let _pending = prepare_dialog(
            DialogKind::Render,
            Some(SaveMovie {
                directory: collision.clone(),
                name: "edit.mp4".into(),
            }),
            &waker,
            &mut directory,
        )
        .unwrap();
        assert!(
            directory
                .take()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .is_err()
        );
        assert_eq!(std::fs::read(collision).unwrap(), b"keep this movie");
    }

    #[test]
    fn pending_dialog_rejects_another_and_cancellation_releases_it() {
        let context = egui::Context::default();
        let mut dialogs = Dialogs {
            pending: Some(PendingDialog {
                kind: DialogKind::ImportMedia,
                future: Box::pin(std::future::pending()),
                waker: Waker::from(Arc::new(Repaint(context.clone()))),
                directory: None,
            }),
            #[cfg(feature = "ui-harness")]
            scripted: None,
        };
        assert!(dialogs.is_open());
        assert!(dialogs.take_result().is_none());
        assert!(dialogs.start(DialogKind::CreateProject, &context).is_err());

        dialogs.pending.as_mut().unwrap().future = Box::pin(std::future::ready(None));
        let result = dialogs.take_result().unwrap();
        assert_eq!(result.kind, DialogKind::ImportMedia);
        assert!(result.path.is_none());
        assert!(!dialogs.is_open());
        assert!(dialogs.take_result().is_none());
    }

    #[test]
    fn new_project_returns_the_selected_original_once_without_changing_its_path() {
        let mut dialogs = Dialogs {
            pending: Some(PendingDialog {
                kind: DialogKind::CreateProject,
                future: Box::pin(std::future::ready(Some(PathBuf::from(
                    "/clips/My interview.mp4",
                )))),
                waker: Waker::from(Arc::new(Repaint(egui::Context::default()))),
                directory: None,
            }),
            #[cfg(feature = "ui-harness")]
            scripted: None,
        };
        let result = dialogs.take_result().unwrap();
        assert_eq!(result.kind, DialogKind::CreateProject);
        assert_eq!(result.path, Some(PathBuf::from("/clips/My interview.mp4")));
        assert!(!dialogs.is_open());
        assert!(dialogs.take_result().is_none());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unsupported_platform_does_not_claim_an_open_dialog() {
        let mut dialogs = Dialogs::default();
        let error = dialogs
            .start(DialogKind::OpenProject, &egui::Context::default())
            .unwrap_err();
        assert!(error.contains("macOS"));
        assert!(!dialogs.is_open());
    }
}
