//! Named Original moments in the Original rail: save the selected moment,
//! filter by name and tag, and select one in the Original. Browsing never
//! edits; every catalog write goes to the project owner.

use std::ops::Range;

use super::*;
use crate::project::moments::{Operation, Request, Update};
use deadpan_store::moments::{
    Action as MomentAction, Moment, MomentCatalog, MomentId, MomentName, MomentTags,
};

pub(super) const FILTER_ID: &str = "moment-filter";

/// What a moment command acts on, captured when command entry opens.
#[derive(Clone, Debug)]
pub(super) struct Capture {
    session: Option<u64>,
    /// The Original's nonempty selection, only while Original is shown.
    selection: Option<Range<u64>>,
    selected: Option<MomentId>,
}

#[derive(Default)]
pub(super) struct State {
    session: Option<u64>,
    catalog: Option<MomentCatalog>,
    pub filter: String,
    selected: Option<MomentId>,
    queued: Option<Request>,
    pending: Option<(u64, u64)>,
    /// Feedback and selection for the queued or pending write, applied when
    /// its own receipt arrives.
    outcome: Option<(String, Option<MomentId>)>,
    pub command: Option<Capture>,
    /// Why the catalog could not be listed, shown in place of the list.
    unavailable: Option<String>,
    reveal: bool,
    /// Focus the filter when it is next drawn; command entry closes after
    /// this frame's drawing and would otherwise take focus back.
    focus_filter: bool,
}

impl State {
    fn loading(&self) -> bool {
        self.queued.is_some() || self.pending.is_some()
    }

    /// Moments the filter admits, in Original order.
    fn visible(&self) -> Vec<Moment> {
        let mut entries: Vec<_> = self
            .catalog
            .iter()
            .flat_map(|catalog| &catalog.entries)
            .filter(|moment| matches(moment, &self.filter))
            .cloned()
            .collect();
        entries.sort_by(|a, b| (a.start, a.end, &a.name).cmp(&(b.start, b.end, &b.name)));
        entries
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn ready_for_check(&self) -> bool {
        self.catalog.is_some() && !self.loading()
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn selected_name_for_check(&self) -> Option<&str> {
        self.catalog
            .as_ref()?
            .entries
            .iter()
            .find(|moment| Some(&moment.id) == self.selected.as_ref())
            .map(|moment| moment.name.as_str())
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn visible_names_for_check(&self) -> Vec<String> {
        self.visible()
            .into_iter()
            .map(|moment| moment.name.as_str().to_owned())
            .collect()
    }
}

/// Every whitespace-separated term must match: `tag:x` names one exact tag;
/// other terms match the name or a tag as a case-insensitive subsequence.
pub(super) fn matches(moment: &Moment, filter: &str) -> bool {
    filter.split_whitespace().all(|term| {
        let term = term.to_lowercase();
        match term.strip_prefix("tag:") {
            Some(tag) => moment.tags.contains(tag),
            None => {
                subsequence(&term, &moment.name.as_str().to_lowercase())
                    || moment
                        .tags
                        .as_slice()
                        .iter()
                        .any(|tag| subsequence(&term, tag.as_str()))
            }
        }
    })
}

fn subsequence(needle: &str, haystack: &str) -> bool {
    let mut haystack = haystack.chars();
    needle
        .chars()
        .all(|wanted| haystack.any(|found| found == wanted))
}

/// Measured Original time of a half-open picture range, from source PTS.
fn seconds(index: &deadpan_core::SourceFrameIndex, range: &Range<u64>) -> Option<f64> {
    let boundary = |ordinal: u64| {
        let ordinal = usize::try_from(ordinal).ok()?;
        match index.frames().get(ordinal) {
            Some(frame) => Some(frame.pts),
            None if ordinal == index.frames().len() => Some(index.terminal_end()),
            None => None,
        }
    };
    let ticks = i128::from(boundary(range.end)?) - i128::from(boundary(range.start)?);
    let base = index.time_base();
    Some(ticks as f64 * f64::from(base.numerator()) / f64::from(base.denominator()))
}

fn duration_text(seconds: Option<f64>, range: &Range<u64>) -> String {
    match seconds {
        Some(seconds) if seconds < 10.0 => format!("{seconds:.2} s"),
        Some(seconds) if seconds < 60.0 => format!("{seconds:.1} s"),
        Some(seconds) => format!("{}:{:04.1}", (seconds / 60.0).floor(), seconds % 60.0),
        None => format!("{} frames", range.end - range.start),
    }
}

impl DeadpanApp {
    fn original_index(&self) -> Option<Arc<deadpan_core::SourceFrameIndex>> {
        let workspace = self.workspace.as_ref()?;
        let asset = original_asset(workspace)?;
        workspace.sources.get(asset)?.video_index.clone()
    }

    pub(super) fn capture_moment_command(&self) -> Capture {
        Capture {
            session: self.workspace.as_ref().map(|workspace| workspace.session),
            selection: (self.view == View::Source && self.moment_identity().is_some())
                .then(|| self.moment.range())
                .flatten(),
            selected: self.moments.selected.clone(),
        }
    }

    pub(super) fn receive_moments(
        &mut self,
        update: Option<Update>,
        catalog: Option<MomentCatalog>,
    ) {
        if let Some(update) = update
            && self.moments.pending == Some((update.session, update.ticket))
        {
            self.moments.pending = None;
            let outcome = self.moments.outcome.take();
            match update.result {
                Ok(receipt) => {
                    self.moments.unavailable = None;
                    self.adopt_moment_catalog(receipt.catalog);
                    if let Some((message, select)) = outcome
                        && receipt.changed
                    {
                        if select.is_some() {
                            self.moments.selected = select;
                            self.moments.reveal = true;
                        }
                        self.error = None;
                        self.message = Some(message);
                    }
                }
                Err(error) if outcome.is_some() => self.error = Some(error),
                Err(error) => self.moments.unavailable = Some(error),
            }
        }
        // Command-line writes publish the catalog without a native ticket.
        if let Some(catalog) = catalog
            && self.moments.catalog.as_ref().is_some_and(|current| {
                current.project_id == catalog.project_id && catalog.version > current.version
            })
        {
            self.adopt_moment_catalog(catalog);
        }
    }

    fn adopt_moment_catalog(&mut self, catalog: MomentCatalog) {
        if self.moments.catalog.as_ref().is_some_and(|current| {
            current.project_id == catalog.project_id && current.version > catalog.version
        }) {
            return;
        }
        if self
            .moments
            .selected
            .as_ref()
            .is_some_and(|id| !catalog.entries.iter().any(|moment| &moment.id == id))
        {
            self.moments.selected = None;
        }
        self.moments.catalog = Some(catalog);
    }

    /// Loads the catalog for each new session and submits one queued request
    /// when the owner is free.
    pub(super) fn dispatch_moments(&mut self) {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        if self.moments.session != session {
            let filter = std::mem::take(&mut self.moments.filter);
            self.moments = State {
                session,
                filter,
                ..Default::default()
            };
            if session.is_some() {
                self.queue_moments(Operation::List, None);
            }
        }
        if self.service.is_busy() {
            return;
        }
        let Some(request) = self.moments.queued.take() else {
            return;
        };
        let identity = (request.session, request.ticket);
        match self.service.submit(ProjectRequest::Moments(request)) {
            Ok(()) => self.moments.pending = Some(identity),
            Err(error) => {
                if self.moments.outcome.take().is_some() {
                    self.error = Some(error);
                }
            }
        }
    }

    fn queue_moments(&mut self, operation: Operation, outcome: Option<(String, Option<MomentId>)>) {
        let Some(ticket) = self.next_serial() else {
            return;
        };
        let Some(session) = self.moments.session else {
            return;
        };
        self.moments.queued = Some(Request {
            ticket,
            session,
            operation,
        });
        self.moments.outcome = outcome;
    }

    /// Check the command-entry capture, then queue `build`'s write against
    /// the catalog version this UI shows.
    fn moment_write(
        &mut self,
        build: impl FnOnce(&Self, &Capture) -> Result<(MomentAction, String, Option<MomentId>), String>,
    ) {
        let capture = self.moments.command.take();
        let result = (|| {
            let capture = capture.ok_or("Open the command again to capture its moment.")?;
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            if capture.session != Some(workspace.session) {
                return Err(
                    "The project changed after the command opened; nothing was saved.".to_owned(),
                );
            }
            if workspace.read_only.is_some() {
                return Err("This project is open read-only; nothing was saved.".into());
            }
            if self.moments.loading() {
                return Err("Wait for the previous moment change to finish.".into());
            }
            let catalog = self
                .moments
                .catalog
                .as_ref()
                .ok_or("Named moments are still loading.")?;
            let (action, message, select) = build(self, &capture)?;
            Ok((
                deadpan_store::moments::Request {
                    project_id: catalog.project_id.clone(),
                    expected_version: catalog.version,
                    action,
                },
                message,
                select,
            ))
        })();
        match result {
            Ok((request, message, select)) => {
                self.queue_moments(Operation::Apply(request), Some((message, select)))
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn save_moment(&mut self, name: MomentName, tags: MomentTags) {
        self.moment_write(|app, capture| {
            let range = capture.selection.clone().ok_or_else(|| {
                format!(
                    "Select a nonempty Original range first: :source, then {} and move. No moment was saved.",
                    app.editor_key(EditorKey::Visual)
                )
            })?;
            let id = MomentId::new(uuid::Uuid::new_v4().to_string())
                .map_err(|error| error.to_string())?;
            let message =
                format!("Saved moment “{name}”. The Original and Your edit are unchanged.");
            Ok((
                MomentAction::Create {
                    id: id.clone(),
                    name,
                    tags,
                    start: range.start,
                    end: range.end,
                },
                message,
                Some(id),
            ))
        });
    }

    fn selected_capture(&self, capture: &Capture) -> Result<Moment, String> {
        let id = capture
            .selected
            .as_ref()
            .ok_or("Select a moment in MOMENTS first (:moments, then ↑/↓).")?;
        self.moments
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.entries.iter().find(|moment| &moment.id == id))
            .cloned()
            .ok_or_else(|| "The selected moment no longer exists.".into())
    }

    pub(super) fn rename_moment(&mut self, name: MomentName) {
        self.moment_write(|app, capture| {
            let moment = app.selected_capture(capture)?;
            let message = format!("Renamed moment “{}” to “{name}”.", moment.name);
            Ok((
                MomentAction::Rename {
                    id: moment.id,
                    name,
                },
                message,
                None,
            ))
        });
    }

    pub(super) fn retag_moment(&mut self, tags: MomentTags) {
        self.moment_write(|app, capture| {
            let moment = app.selected_capture(capture)?;
            let message = if tags.as_slice().is_empty() {
                format!("Cleared the tags of “{}”.", moment.name)
            } else {
                format!("Tagged “{}”: {}.", moment.name, tag_list(&tags))
            };
            Ok((
                MomentAction::Retag {
                    id: moment.id,
                    tags,
                },
                message,
                None,
            ))
        });
    }

    pub(super) fn delete_moment(&mut self) {
        self.moment_write(|app, capture| {
            let moment = app.selected_capture(capture)?;
            Ok((
                MomentAction::Delete { id: moment.id },
                format!(
                    "Deleted moment “{}”. The Original and Your edit are unchanged.",
                    moment.name
                ),
                None,
            ))
        });
    }

    /// `:moments [filter]`: focus the filter so the list is operable by keys.
    pub(super) fn focus_moments(&mut self, filter: Option<String>, context: &egui::Context) {
        let ready = self.workspace.as_ref().is_some_and(|workspace| {
            matches!(
                workspace.single_source,
                Some(SingleSourceState::Ready { .. })
            )
        });
        if !ready {
            self.error = Some("Named moments belong to a project whose Original is ready.".into());
            return;
        }
        if let Some(filter) = filter {
            self.moments.filter = filter;
        }
        self.focus_scoped_pane(Pane::Sources);
        self.moments.reveal = true;
        let mut text = egui::text_edit::TextEditState::default();
        text.cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(self.moments.filter.chars().count()),
            )));
        text.store(context, egui::Id::new(FILTER_ID));
        self.moments.focus_filter = true;
        context.request_repaint();
        self.message = Some(
            "Type to filter by name, or tag:reaction. ↑/↓ choose; Enter selects it in Original; Esc returns."
                .into(),
        );
    }

    /// ↑/↓ and Enter while the filter is focused. Returns whether `key` was used.
    pub(super) fn moment_filter_key(&mut self, key: egui::Key, context: &egui::Context) -> bool {
        let visible = self.moments.visible();
        let current = visible
            .iter()
            .position(|moment| Some(&moment.id) == self.moments.selected.as_ref());
        match key {
            egui::Key::ArrowDown | egui::Key::ArrowUp => {
                if visible.is_empty() {
                    return true;
                }
                let next = match (current, key == egui::Key::ArrowDown) {
                    (None, true) => 0,
                    (None, false) => visible.len() - 1,
                    (Some(index), true) => (index + 1).min(visible.len() - 1),
                    (Some(index), false) => index.saturating_sub(1),
                };
                self.moments.selected = Some(visible[next].id.clone());
                self.moments.reveal = true;
                true
            }
            egui::Key::Enter => {
                let chosen = current.or((!visible.is_empty()).then_some(0));
                match chosen {
                    Some(index) => {
                        let moment = visible[index].clone();
                        self.open_moment(&moment);
                        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
                    }
                    None => self.error = Some("No moment matches this filter.".into()),
                }
                true
            }
            _ => false,
        }
    }

    /// Select `moment` in the Original: its range and the cursor at its In.
    /// Never inserts or edits.
    fn open_moment(&mut self, moment: &Moment) {
        let Some(asset) = self
            .workspace
            .as_ref()
            .and_then(|workspace| original_asset(workspace))
            .cloned()
        else {
            self.error = Some("The Original is not ready.".into());
            return;
        };
        self.moments.selected = Some(moment.id.clone());
        self.select_source(asset);
        if moment.end > self.source_length() {
            self.error = Some("This moment ends after the Original's measured pictures.".into());
            return;
        }
        self.moment.select(moment.start..moment.end);
        self.source_cursor = moment.start;
        self.request_picture(false);
        self.error = None;
        let length = self
            .original_index()
            .and_then(|index| seconds(&index, &(moment.start..moment.end)));
        self.message = Some(format!(
            "Moment “{}” selected in Original: In {} · Out {} excluded · {}. {} copies it; nothing was inserted.",
            moment.name,
            moment.start,
            moment.end,
            duration_text(length, &(moment.start..moment.end)),
            self.editor_key(EditorKey::Copy)
        ));
    }

    /// The MOMENTS section of the Original rail. One scoped ID keeps later
    /// rail widgets' automatic IDs fixed as the list loads, grows or empties.
    pub(super) fn moments_section(&mut self, ui: &mut egui::Ui) {
        ui.push_id("moments-section", |ui| self.moments_rows(ui));
    }

    fn moments_rows(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.label(style::section_title("MOMENTS", false));
        let hint = "Filter moments  :moments";
        let filter = ui.add(
            egui::TextEdit::singleline(&mut self.moments.filter)
                .id(egui::Id::new(FILTER_ID))
                .hint_text(hint)
                .char_limit(160)
                .desired_width(f32::INFINITY),
        );
        accessibility::name(&filter, "Filter moments");
        if std::mem::take(&mut self.moments.focus_filter) {
            filter.request_focus();
        }
        if filter.has_focus() {
            self.focus_scoped_pane(Pane::Sources);
        }
        retain_text_escape(ui, FILTER_ID);
        let index = self.original_index();
        let visible = self.moments.visible();
        if self.moments.catalog.is_none() {
            match &self.moments.unavailable {
                Some(error) => {
                    ui.colored_label(style::ERROR, format!("Moments unavailable: {error}"));
                }
                None => {
                    ui.weak("Loading moments…");
                }
            }
            return;
        }
        if visible.is_empty() {
            let empty = self
                .moments
                .catalog
                .as_ref()
                .is_some_and(|catalog| catalog.entries.is_empty());
            ui.weak(if empty {
                "No moments yet. Select time in Original, then :moment NAME."
            } else {
                "No matching moments."
            });
            return;
        }
        let reveal = std::mem::take(&mut self.moments.reveal);
        let mut opened = None;
        egui::ScrollArea::vertical()
            .id_salt("moment-list")
            .max_height(156.0)
            .show(ui, |ui| {
                for moment in &visible {
                    let range = moment.start..moment.end;
                    let length = duration_text(
                        index.as_deref().and_then(|index| seconds(index, &range)),
                        &range,
                    );
                    let tags = tag_list(&moment.tags);
                    let detail = if tags.is_empty() {
                        length.clone()
                    } else {
                        format!("{length} · {tags}")
                    };
                    let selected = self.moments.selected.as_ref() == Some(&moment.id);
                    let mut job = egui::text::LayoutJob::default();
                    let body = egui::TextStyle::Body.resolve(ui.style());
                    job.append(
                        moment.name.as_str(),
                        0.0,
                        egui::TextFormat {
                            font_id: body.clone(),
                            color: ui.visuals().strong_text_color(),
                            ..Default::default()
                        },
                    );
                    job.append(
                        &format!("\n{detail}"),
                        0.0,
                        egui::TextFormat {
                            font_id: egui::FontId::new(11.0, body.family.clone()),
                            color: ui.visuals().weak_text_color(),
                            ..Default::default()
                        },
                    );
                    let response = ui
                        .push_id(moment.id.as_str(), |ui| {
                            ui.add(
                                egui::Button::selectable(selected, (job, egui::Atom::grow()))
                                    .min_size(egui::vec2(ui.available_width(), 0.0))
                                    .wrap(),
                            )
                        })
                        .inner;
                    accessibility::name(&response, &format!("Moment {}, {detail}", moment.name));
                    if selected && reveal {
                        response.scroll_to_me(None);
                    }
                    if response.clicked() {
                        opened = Some(moment.clone());
                    }
                }
            });
        if let Some(moment) = opened {
            self.open_moment(&moment);
            ui.memory_mut(|memory| memory.request_focus(pane_id(Pane::Sources)));
        }
    }
}

fn tag_list(tags: &MomentTags) -> String {
    tags.as_slice()
        .iter()
        .map(|tag| tag.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment(name: &str, tags: &[&str]) -> Moment {
        Moment {
            id: MomentId::new(name.replace(' ', "-")).unwrap(),
            name: MomentName::new(name).unwrap(),
            tags: MomentTags::from_input(tags.iter().copied()).unwrap(),
            start: 0,
            end: 1,
        }
    }

    #[test]
    fn filters_match_name_subsequences_tags_and_exact_tag_terms() {
        let laugh = moment("Big laugh", &["reaction"]);
        let sigh = moment("Sigh", &["breath", "reactionary"]);
        assert!(matches(&laugh, ""));
        assert!(matches(&laugh, "blgh"));
        assert!(matches(&laugh, "BIG"));
        assert!(!matches(&laugh, "hgl"));
        assert!(matches(&sigh, "brth"), "tags match fuzzily too");
        assert!(matches(&laugh, "tag:reaction"));
        assert!(!matches(&sigh, "tag:reaction"), "tag: is exact");
        assert!(matches(&sigh, "tag:Breath sig"));
        assert!(!matches(&sigh, "tag:breath laugh"), "every term must match");
    }

    #[test]
    fn duration_text_uses_seconds_and_minutes() {
        assert_eq!(duration_text(Some(0.467), &(0..3)), "0.47 s");
        assert_eq!(duration_text(Some(12.25), &(0..3)), "12.2 s");
        assert_eq!(duration_text(Some(75.0), &(0..3)), "1:15.0");
        assert_eq!(duration_text(None, &(4..9)), "5 frames");
    }
}
