//! One native dialog, polled by the live application's event loop.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    CreateProject,
    OpenProject,
    ImportMedia,
}

pub struct DialogResult {
    pub kind: DialogKind,
    pub path: Option<PathBuf>,
}

type DialogFuture = Pin<Box<dyn Future<Output = Option<PathBuf>> + Send>>;

struct PendingDialog {
    kind: DialogKind,
    future: DialogFuture,
    waker: Waker,
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
}

impl Dialogs {
    /// Call only from the main thread during an update of the live window.
    /// Native sheet creation happens here; subsequent polls only inspect its result.
    pub fn start(&mut self, kind: DialogKind, context: &egui::Context) -> Result<(), String> {
        if self.is_open() {
            return Err("Finish or cancel the open dialog before opening another.".into());
        }
        let future = native_dialog(kind)?;
        self.pending = Some(PendingDialog {
            kind,
            future,
            waker: Waker::from(Arc::new(Repaint(context.clone()))),
        });
        // Register the completion waker on the next update, even if the UI is idle.
        context.request_repaint();
        Ok(())
    }

    pub fn take_result(&mut self) -> Option<DialogResult> {
        let pending = self.pending.as_mut()?;
        let mut context = Context::from_waker(&pending.waker);
        let Poll::Ready(path) = pending.future.as_mut().poll(&mut context) else {
            return None;
        };
        let kind = pending.kind;
        self.pending = None;
        Some(DialogResult {
            kind,
            path: path.map(|path| match kind {
                DialogKind::CreateProject => project_path(path),
                DialogKind::OpenProject | DialogKind::ImportMedia => path,
            }),
        })
    }

    pub fn is_open(&self) -> bool {
        self.pending.is_some()
    }
}

fn project_path(path: PathBuf) -> PathBuf {
    if path.extension() == Some(std::ffi::OsStr::new("deadpan")) {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".deadpan");
        PathBuf::from(name)
    }
}

#[cfg(target_os = "macos")]
fn native_dialog(kind: DialogKind) -> Result<DialogFuture, String> {
    // Construct on the main thread so rfd attaches an asynchronous sheet to the
    // running NSApplication. Moving construction into an async block would defer
    // that requirement to whoever first polls it.
    let future: Pin<Box<dyn Future<Output = Option<rfd::FileHandle>> + Send>> = match kind {
        DialogKind::CreateProject => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Create Deadpan project (.deadpan)")
                .set_file_name("Untitled.deadpan")
                .set_can_create_directories(true)
                .save_file(),
        ),
        DialogKind::OpenProject => Box::pin(
            rfd::AsyncFileDialog::new()
                .set_title("Open a Deadpan project (.deadpan)")
                .set_can_create_directories(false)
                .pick_folder(),
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
fn native_dialog(_kind: DialogKind) -> Result<DialogFuture, String> {
    Err("Native file dialogs are supported only on macOS.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_names_preserve_authored_suffixes_and_end_in_deadpan() {
        assert_eq!(
            project_path(PathBuf::from("/projects/Untitled")),
            PathBuf::from("/projects/Untitled.deadpan")
        );
        for name in ["A.deadpan", "with spaces.deadpan"] {
            let path = PathBuf::from("/projects").join(name);
            assert_eq!(project_path(path.clone()), path);
        }
        for name in ["A.custom", "A.", "A.DEADPAN", "with spaces.edit"] {
            assert_eq!(
                project_path(PathBuf::from(name)),
                PathBuf::from(format!("{name}.deadpan"))
            );
        }
        assert_eq!(
            project_path(PathBuf::from("/projects/.hidden")),
            PathBuf::from("/projects/.hidden.deadpan")
        );
    }

    #[test]
    fn pending_dialog_rejects_another_and_cancellation_releases_it() {
        let context = egui::Context::default();
        let mut dialogs = Dialogs {
            pending: Some(PendingDialog {
                kind: DialogKind::ImportMedia,
                future: Box::pin(std::future::pending()),
                waker: Waker::from(Arc::new(Repaint(context.clone()))),
            }),
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
    fn chosen_project_path_is_returned_once_with_its_extension() {
        let mut dialogs = Dialogs {
            pending: Some(PendingDialog {
                kind: DialogKind::CreateProject,
                future: Box::pin(std::future::ready(Some(PathBuf::from("/projects/My cut")))),
                waker: Waker::from(Arc::new(Repaint(egui::Context::default()))),
            }),
        };
        let result = dialogs.take_result().unwrap();
        assert_eq!(result.kind, DialogKind::CreateProject);
        assert_eq!(result.path, Some(PathBuf::from("/projects/My cut.deadpan")));
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
