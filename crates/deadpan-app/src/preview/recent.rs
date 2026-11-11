//! Recent projects on the start screen: the newest packages in the project
//! library, each opened with a number key. Listing reads only directory
//! metadata, once when the start screen appears; opening uses the ordinary
//! project Open.

use std::path::PathBuf;
use std::time::SystemTime;

use super::*;

/// How many recent projects the start screen offers, keyed 1 to 5.
const SHOWN: usize = 5;

#[derive(Default)]
pub(super) struct Recent {
    /// Listed since the start screen last appeared.
    listed: bool,
    pub(super) projects: Vec<(PathBuf, String, SystemTime)>,
}

impl Recent {
    /// Forget the list while a project is open, so the next start screen
    /// lists the library afresh.
    pub(super) fn forget(&mut self) {
        self.listed = false;
    }

    fn list(&mut self, root: Option<PathBuf>) {
        if self.listed {
            return;
        }
        self.listed = true;
        self.projects.clear();
        let Some(Ok(entries)) = root.map(std::fs::read_dir) else {
            return;
        };
        let mut projects: Vec<_> = entries
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "deadpan")
            })
            .filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                let name = entry.path().file_stem()?.to_string_lossy().into_owned();
                Some((entry.path(), name, modified))
            })
            .collect();
        projects.sort_by_key(|project| std::cmp::Reverse(project.2));
        projects.truncate(SHOWN);
        self.projects = projects;
    }
}

/// "today", "yesterday" or "N days ago", from a modification time.
fn age(modified: SystemTime) -> String {
    let days = SystemTime::now()
        .duration_since(modified)
        .map_or(0, |elapsed| elapsed.as_secs() / 86_400);
    match days {
        0 => "today".into(),
        1 => "yesterday".into(),
        days => format!("{days} days ago"),
    }
}

impl DeadpanApp {
    /// The start screen's recent-project rows.
    pub(super) fn recent_projects(&mut self, ui: &mut egui::Ui) {
        let root = self.youtube.jobs.library_root();
        self.recent.list(root);
        if self.recent.projects.is_empty() {
            return;
        }
        ui.label(style::section_title("RECENT PROJECTS", false));
        let mut open = None;
        for (index, (path, name, modified)) in self.recent.projects.iter().enumerate() {
            let key = (index + 1).to_string();
            let text = style::action_text(&format!("{name}  ·  {}", age(*modified)), &key, 13.0);
            let response = ui.add(
                egui::Button::new((text, egui::Atom::grow()))
                    .min_size(egui::vec2(ui.available_width(), 28.0)),
            );
            if response.on_hover_text(path.display().to_string()).clicked() {
                open = Some(index);
            }
        }
        if let Some(index) = open {
            self.open_recent(index);
        }
    }

    /// Plain 1 to 5 on the start screen open a recent project.
    pub(super) fn recent_keyboard(&mut self, context: &egui::Context) -> bool {
        if self.workspace.is_some() {
            self.recent.forget();
            return false;
        }
        if self.raw_source.is_some()
            || self.help_open
            || self.dialogs.is_open()
            || self.command_open
            || text_input_active(context, false)
        {
            return false;
        }
        let keys = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
        ];
        let Some(index) = keys.iter().position(|key| {
            context.input(|input| input.key_pressed(*key) && input.modifiers.is_none())
        }) else {
            return false;
        };
        if index >= self.recent.projects.len() {
            return false;
        }
        context.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, keys[index]);
            input.events.retain(|event| !matches!(event, egui::Event::Text(text) if text == &(index + 1).to_string()));
        });
        self.open_recent(index);
        true
    }

    fn open_recent(&mut self, index: usize) {
        let Some((path, ..)) = self.recent.projects.get(index).cloned() else {
            return;
        };
        if self.service.is_busy() || self.importing() {
            self.error = Some(
                "Wait for the current project work to finish before opening another project."
                    .into(),
            );
            return;
        }
        self.message = Some(format!(
            "Opening {}…",
            path.file_stem()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        ));
        self.submit(ProjectRequest::Open(path));
    }
}
