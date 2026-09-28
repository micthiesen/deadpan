//! Drives production `DeadpanApp::ui`, with real project/media services and Metal.
//! Only OS picker results and explicitly requested delivery faults are scripted.

use std::path::Path;
use std::time::Instant;

use egui_kittest::{Harness, kittest::NodeT as _};
use serde_json::{Value, json};

use super::*;
use crate::ui_harness::{Options, gpu::Offscreen, report::*};

mod edit_latency;
mod gain;
mod moment;
mod nested_pause;
mod original_playback;
mod repeat_input;
mod retime;
mod room_tone;
mod scale;
mod scenarios;
mod sound_placement;
mod sound_playback;
mod telemetry;
mod wake;

use telemetry::{Event, InputOrigins, Outcome, PictureTelemetry};
use wake::RepaintWake;

#[derive(Default)]
pub(super) struct Feedback {
    pub footer_bottom: Option<(f32, f32)>,
    pub footer_command_open: bool,
    pub hold_project_updates: bool,
    pub hold_preview: bool,
    pub held_reply: Option<crate::worker::Reply>,
    pub release_reply: Option<crate::worker::Reply>,
    pub fail_next_preview: bool,
    pub simulate_playback: bool,
    pub playback_updates: std::collections::VecDeque<deadpan_playback::Update>,
    events: Vec<Event<Ticket>>,
}

impl Feedback {
    pub fn take_project_update(
        &self,
        service: &ProjectService,
    ) -> Option<crate::project::ProjectUpdate> {
        if self.hold_project_updates {
            None
        } else {
            service.take_update()
        }
    }

    pub fn record(&mut self, stage: &'static str) {
        self.record_event(stage, None, None, None);
    }

    fn record_event(
        &mut self,
        stage: &'static str,
        ticket: Option<Ticket>,
        outcome: Option<Outcome>,
        worker_timing: Option<crate::worker::WorkerTiming>,
    ) {
        // Diagnostics must never introduce an unbounded queue in the app.
        if self.events.len() < 4096 {
            self.events.push(Event {
                stage,
                at: Instant::now(),
                ticket,
                outcome,
                request_elapsed_ms: None,
                worker_timing,
            });
        }
    }

    pub fn picture_requested(&mut self, ticket: Ticket) {
        self.record_event(
            "picture_requested",
            Some(ticket),
            Some(Outcome::Requested),
            None,
        );
    }

    pub fn picture_received(
        &mut self,
        ticket: Ticket,
        accepted_success: Option<bool>,
        timing: Option<crate::worker::WorkerTiming>,
    ) {
        let outcome = match accepted_success {
            Some(true) => Outcome::Decoded,
            Some(false) => Outcome::Failed,
            None => Outcome::Stale,
        };
        self.record_event("picture_received", Some(ticket), Some(outcome), timing);
    }

    pub fn picture_submitted(&mut self, ticket: Option<Ticket>) {
        if let Some(ticket) = ticket {
            self.record_event(
                "picture_submitted",
                Some(ticket),
                Some(Outcome::Submitted),
                None,
            );
        }
    }

    pub fn picture_failed(&mut self, ticket: Option<Ticket>) {
        if let Some(ticket) = ticket {
            self.record_event(
                "picture_render_failed",
                Some(ticket),
                Some(Outcome::Failed),
                None,
            );
        }
    }

    pub fn take_reply(&mut self, worker: &PreviewWorker) -> Option<crate::worker::Reply> {
        if self.hold_preview {
            if self.held_reply.is_none() {
                self.held_reply = worker.take_reply();
            }
            return None;
        }
        let mut reply = self.release_reply.take().or_else(|| worker.take_reply())?;
        if self.fail_next_preview {
            self.fail_next_preview = false;
            reply.picture = Err("UI replay injected a decoder failure".into());
        }
        Some(reply)
    }
}

pub(crate) fn run(name: &str, options: &Options, fixture: &Path) -> ScenarioReport {
    let mut report = ScenarioReport::new(name);
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
            let retained = options.retained_project_root(name)?;
            let scratch = if retained.is_none() {
                Some(tempfile::Builder::new().prefix("deadpan-ui-").tempdir().map_err(|error| error.to_string())?)
            } else { None };
            let documents = if let Some(root) = retained {
                let parent = root.parent().ok_or("Retained replay root has no parent")?;
                std::fs::create_dir_all(parent).map_err(|error| format!("Create retained project parent {}: {error}", parent.display()))?;
                std::fs::create_dir(&root).map_err(|error| format!("Create exclusive retained scenario root {}: {error}", root.display()))?;
                let documents = root.join("Documents");
                std::fs::create_dir(&documents).map_err(|error| format!("Create retained Documents root {}: {error}", documents.display()))?;
                let documents = documents.canonicalize().map_err(|error| format!("Resolve retained Documents root {}: {error}", documents.display()))?;
                report.checks.push(Check {
                    name: "Private replay project root retained for native QA after worker shutdown".into(),
                    passed: true,
                    expected: json!({"scenario":name,"exclusive_creation":true,"cleanup_on_exit":false}),
                    actual: json!({"scenario":name,"documents_root":documents,"exclusive_creation":true,"cleanup_on_exit":false}),
                });
                documents
            } else {
                scratch.as_ref().ok_or("Missing temporary replay root")?.path().join("Documents")
            };
            let library = crate::library::ProjectLibrary::from_documents(documents)?;
            let mut construction_error = None;
            let wake = Arc::new(RepaintWake::default());
            // The wrapper lets construction failures become evidence rather than a
            // panic that skips the scenario report.
            let harness = Harness::builder()
                .with_size(egui::vec2(1280.0, 820.0))
                .with_os(egui::os::OperatingSystem::Mac)
                .with_pixels_per_point(1.0)
                .with_step_dt(1.0 / options.hz as f32)
                .with_wait_for_pending_images(false)
                .with_render_options(egui_wgpu::RendererOptions::default())
                .wgpu()
                .build_eframe(|context| {
                    // kittest owns this fresh context; native eframe owns its
                    // own callback, which must never be replaced by the app.
                    wake.install(&context.egui_ctx);
                    let result = (|| {
                        let render = context
                            .wgpu_render_state
                            .clone()
                            .ok_or("Missing offscreen GPU")?;
                        if render.adapter.get_info().backend != eframe::wgpu::Backend::Metal {
                            return Err("UI qualification requires the Metal backend".to_owned());
                        }
                        let mut app = DeadpanApp::new(
                            context,
                            render,
                            false,
                            Rc::new(Cell::new(false)),
                            None,
                            None,
                        )
                        .map_err(|e| e.to_string())?;
                        let repaint = context.egui_ctx.clone();
                        app.service = ProjectService::start(
                            Arc::new(move || repaint.request_repaint()),
                            Some(library),
                        )
                        .map_err(|e| e.to_string())?;
                        app.dialogs = Dialogs::scripted(vec![(
                            DialogKind::CreateProject,
                            Some(fixture.to_owned()),
                        )]);
                        app.feedback.simulate_playback = true;
                        Ok(app)
                    })();
                    match result {
                        Ok(app) => ReplayApp(Some(app)),
                        Err(error) => {
                            construction_error = Some(error);
                            ReplayApp(None)
                        }
                    }
                });
            if let Some(error) = construction_error {
                return Err(error);
            }
            // kittest's static-snapshot defaults suppress dynamics. Restore the
            // production settings before observing any scenario input.
            harness.ctx.all_styles_mut(|style| {
                let defaults = egui::Style::default();
                style.scroll_animation = defaults.scroll_animation;
                style.visuals.text_cursor.blink = defaults.visuals.text_cursor.blink;
            });
            let render = harness.state().app()?.render_state.clone();
            report.checks.push(Check {
                name: "Metal renderer".into(),
                passed: true,
                expected: json!("Metal"),
                actual: json!(render.adapter.get_info().name),
            });
            let mut driver = Driver {
                harness,
                gpu: Offscreen::new(render),
                report: &mut report,
                options,
                started: Instant::now(),
                frame: 0,
                captures: 0,
                capture_limit_reported: false,
                capture_wall: Duration::ZERO,
                picture_telemetry: PictureTelemetry::default(),
                input_origins: InputOrigins::default(),
                last_input: None,
                wake,
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                driver.step("Initial workspace", true)?;
                driver.click("New project  ⌘N")?;
                driver.wait_for("Original initialized and displayed", |app| {
                    app.workspace.as_ref().is_some_and(|w| {
                        matches!(w.single_source, Some(SingleSourceState::Ready { .. }))
                    }) && app.presentation.has_displayed()
                        && !app.presentation.loading()
                })?;
                driver.capture("Original ready")?;
                driver.command("sequence")?;
                driver.settled()?;
                if name == "sound-placement" {
                    sound_placement::run(&mut driver)
                } else if name == "room-tone" {
                    room_tone::run(&mut driver)
                } else if name == "gain" {
                    gain::run(&mut driver)
                } else {
                    scenarios::run(name, &mut driver)
                }
            }))
            .unwrap_or_else(|panic| Err(panic_message(panic)));
            if let Err(error) = &result {
                driver.report.findings.push(Finding {
                    severity: Severity::Failure,
                    message: error.clone(),
                });
                // The final frame is still useful even when the assertion failed.
                if let Err(error) = driver.capture("Failure state") {
                    driver.report.findings.push(Finding {
                        severity: Severity::Failure,
                        message: format!("Failure capture: {error}"),
                    });
                }
            }
            driver.report.skipped.extend([
                "OS picker behavior: only its selected path is scripted".into(),
                "Physical display presentation, VoiceOver, OS IME delivery and audio device output"
                    .into(),
            ]);
            // Drop the app and its writer before cleaning temporary storage.
            // Requested retained project roots remain in the report output.
            drop(driver);
            drop(scratch);
            result
        }))
        .unwrap_or_else(|panic| Err(panic_message(panic)));
    if let Err(error) = result
        && !report
            .findings
            .iter()
            .any(|finding| finding.message == error)
    {
        report.findings.push(Finding {
            severity: Severity::Failure,
            message: error,
        });
    }
    report
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("unknown panic");
    format!("Scenario panicked: {message}")
}

struct ReplayApp(Option<DeadpanApp>);
impl ReplayApp {
    fn app(&self) -> Result<&DeadpanApp, String> {
        self.0
            .as_ref()
            .ok_or_else(|| "App construction failed".into())
    }
}
impl eframe::App for ReplayApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(app) = &mut self.0 {
            app.ui(ui, frame);
        }
    }
}

struct Driver<'a> {
    harness: Harness<'static, ReplayApp>,
    gpu: Offscreen,
    report: &'a mut ScenarioReport,
    options: &'a Options,
    started: Instant,
    frame: u64,
    captures: u32,
    capture_limit_reported: bool,
    capture_wall: Duration,
    picture_telemetry: PictureTelemetry<Ticket>,
    input_origins: InputOrigins,
    last_input: Option<Instant>,
    wake: Arc<RepaintWake>,
}

impl Driver<'_> {
    fn app(&self) -> &DeadpanApp {
        self.harness.state().0.as_ref().expect("constructed app")
    }
    fn app_mut(&mut self) -> &mut DeadpanApp {
        self.harness
            .state_mut()
            .0
            .as_mut()
            .expect("constructed app")
    }

    fn metric(&mut self, name: &str, elapsed_ms: f64, outcome: SampleOutcome) {
        let index = match self
            .report
            .timings
            .iter()
            .position(|metric| metric.name == name)
        {
            Some(index) => index,
            None => {
                self.report.timings.push(TimingMetric {
                    name: name.into(),
                    samples: Vec::new(),
                });
                self.report.timings.len() - 1
            }
        };
        self.report.timings[index].samples.push(TimingSample {
            elapsed_ms,
            outcome,
        });
    }

    fn check(
        &mut self,
        name: &str,
        passed: bool,
        expected: Value,
        actual: Value,
    ) -> Result<(), String> {
        self.report.checks.push(Check {
            name: name.into(),
            passed,
            expected,
            actual,
        });
        if passed {
            Ok(())
        } else {
            Err(format!("Check failed: {name}"))
        }
    }

    fn step(&mut self, input: &str, capture: bool) -> Result<(), String> {
        if self.frame >= 6000 {
            return Err("Scenario exceeded the 6000-frame trace budget".into());
        }
        self.frame += 1;
        self.harness.input_mut().time = Some(self.frame as f64 / f64::from(self.options.hz));
        let has_input = !self.harness.input().events.is_empty();
        self.wake.begin_step(Instant::now());
        let started = Instant::now();
        self.harness.step();
        let cpu_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.wake.finish_step(
            self.harness.ctx.cumulative_pass_nr(),
            self.harness.output().viewport_output[&egui::ViewportId::ROOT].repaint_delay,
            Instant::now(),
        );
        self.metric("ui_frame_cpu_ms", cpu_ms, SampleOutcome::Completed);
        if has_input {
            self.metric("input_to_state_ms", cpu_ms, SampleOutcome::Completed);
        }
        let pending_events = std::mem::take(&mut self.app_mut().feedback.events);
        let mut events = Vec::new();
        self.input_origins
            .begin_frame(if has_input { self.last_input } else { None });
        for event in pending_events {
            if let Some(start) = self.input_origins.observe(event.stage) {
                self.metric(
                    "input_to_commit_ms",
                    event.at.saturating_duration_since(start).as_secs_f64() * 1000.0,
                    SampleOutcome::Completed,
                );
            }
            let observed = self
                .picture_telemetry
                .observe(event, self.input_origins.picture_input());
            for measurement in observed.measurements {
                self.metric(
                    measurement.name,
                    measurement.elapsed_ms,
                    SampleOutcome::Completed,
                );
            }
            events.extend(observed.events);
        }
        let (submit, complete) = self.gpu.submit(&self.harness.ctx, self.harness.output())?;
        self.metric(
            "ui_composition_submit_cpu_ms",
            submit,
            SampleOutcome::Completed,
        );
        self.metric(
            "ui_composition_complete_ms",
            complete,
            SampleOutcome::Completed,
        );
        let observed = self.picture_telemetry.composed(Instant::now());
        for measurement in observed.measurements {
            self.metric(
                measurement.name,
                measurement.elapsed_ms,
                SampleOutcome::Completed,
            );
        }
        events.extend(observed.events);
        let mut semantic = self.snapshot();
        if capture
            && self.options.mode == RunMode::Visual
            && self.captures >= 120
            && !self.capture_limit_reported
        {
            self.capture_limit_reported = true;
            self.report.findings.push(Finding {
                severity: Severity::Warning,
                message: "Intermediate screenshot allowance reached; semantic frames continue and named checkpoints retain reserved image capacity.".into(),
            });
        }
        semantic["stages"] = json!(events.iter().map(|event| json!({
            "stage": event.stage,
            "wall_ms": event.at.saturating_duration_since(self.started).as_secs_f64()*1000.0,
            "ticket": event.ticket.map(|ticket| format!("{ticket:?}")),
            "outcome": event.outcome.map(Outcome::label),
            "request_elapsed_ms": event.request_elapsed_ms,
        })).collect::<Vec<_>>());
        let screenshot = if capture && self.options.mode == RunMode::Visual && self.captures < 120 {
            self.captures += 1;
            let name = format!("{}-{:03}.png", self.report.name, self.captures);
            self.save_frame(&name)?;
            semantic["widgets"] = self.widgets();
            Some(name)
        } else {
            None
        };
        self.report.steps.push(StepReport {
            input: input.into(),
            frame: self.frame,
            virtual_time_ms: self.frame as f64 * 1000.0 / f64::from(self.options.hz),
            elapsed_wall_ms: self.started.elapsed().as_secs_f64() * 1000.0,
            semantic,
            screenshot,
        });
        Ok(())
    }

    fn snapshot(&self) -> Value {
        let app = self.app();
        let mut snapshot = json!({
            "pane":format!("{:?}",app.pane),"context":format!("{:?}",app.view),
            "selected_beat":app.selected_beat,"selected_source":app.selected_source,
            "selected_sound":app.selected_sound,"sound_cursor":app.sound_cursor,
            "source_summary":app.summary.as_ref().map(|summary| json!({"width":summary.info.width,"height":summary.info.height,"frame_count":summary.frame_count,"first_pts":summary.first_pts,"terminal_pts":summary.terminal_pts,"time_base":[summary.info.time_base_num,summary.info.time_base_den]})),
            "sequence_cursor":app.sequence_cursor,"source_cursor":app.source_cursor,
            "original_selection":app.moment.range(),"visual_selection":app.moment.active,
            "copied_moment":app.moment.copied.as_ref().map(|copied| copied.ordinals.clone()),
            "sequence_scope":app.sequence_scope.groups(),"scope_start":app.scope_start,"scope_end":app.scope_end,"scope_labels":app.scope_labels,
            "revision":app.workspace.as_ref().map(|w| w.document.revision_id()),
            "duration":app.sequence_length(),"beat_count":app.beat_rows.len(),
            "busy":app.service.is_busy(),"command_open":app.command_open,"command":app.command,
            "repeat_queue":{"active":app.repeat_queue.active(),"waiting":app.repeat_queue.waiting(),"status":app.repeat_queue.status()},
            "import":app.import.as_ref().map(|status| json!({"stage":format!("{:?}",status.stage),"error":status.error})),
            "help_open":app.help_open,"camera_open":app.camera.is_some(),"camera_pending":app.camera_pending.is_some(),
            "playback": app.transport.as_ref().map(|run| json!({"phase":format!("{:?}",run.phase),"sample":run.sample.0,"content_sample":run.content_sample().ok().map(|sample|sample.0),"looping":run.window().looping(),"window":[run.window().start().0,run.window().end().0],"lap":run.lap().ok(),"domain":match run.domain(){crate::transport::Domain::Sequence{..}=>"sequence",crate::transport::Domain::Original(_)=>"original",crate::transport::Domain::Sound(_)=>"sound",crate::transport::Domain::AudioRange(_)=>"audio-range"},"generation":format!("{:?}",run.generation)})),
            "pending_keys":app.bindings.pending(),"picture":app.presentation.diagnostic_snapshot(),
            "error":app.error,"project_error":app.project_error,"message":app.message,
            "focused_widget":self.harness.ctx.memory(|memory| memory.focused().map(|id| format!("{id:?}"))),
            "viewport_points":[self.harness.ctx.content_rect().width(),self.harness.ctx.content_rect().height()],
            "pixels_per_point":self.harness.ctx.pixels_per_point(),
            "layout_passes":self.harness.output().platform_output.num_completed_passes,
            "footer_bottom":app.feedback.footer_bottom,
            "footer_command_open":app.feedback.footer_command_open,
        });
        snapshot["selected_event"] = json!(app.selected_event);
        snapshot["sound_events"] = json!(app.workspace.as_ref().map(|w| w.document.sounds()));
        snapshot["sound_routes"] = json!(app.workspace.as_ref().map(|w| w.document.sound_routes()));
        snapshot["room_tone"] = json!(app.room_tone.as_ref().map(|draft| json!({
            "prepared": draft.prepared.as_ref().map(|prepared| json!({
                "ticket":prepared.ticket,"session":prepared.session,"revision":prepared.revision,
                "source":prepared.source,"duration_samples":prepared.audition.duration_samples().0,
            })),
        })));
        snapshot
    }

    fn widgets(&self) -> Value {
        json!(self.harness.root().children_recursive().take(2048).map(|node| {
            let access = node.accesskit_node();
            let bounds = access.bounding_box().map(|_| node.rect()).map(|bounds| [bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y]);
            json!({"label":access.label(),"role":format!("{:?}",access.role()),"value":access.value(),"focused":access.is_focused(),"disabled":access.is_disabled(),"hidden":access.is_hidden(),"rect":bounds})
        }).collect::<Vec<_>>())
    }

    fn rect(&self, label: &str) -> Result<egui::Rect, String> {
        let matches = self
            .harness
            .root()
            .children_recursive()
            .filter(|node| {
                let access = node.accesskit_node();
                access.label().as_deref() == Some(label)
                    && !access.is_disabled()
                    && !access.is_hidden()
                    && access.bounding_box().is_some()
                    && node.rect().is_positive()
                    && self
                        .harness
                        .ctx
                        .content_rect()
                        .contains(node.rect().center())
            })
            .map(|node| node.rect())
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return Ok(matches[0]);
        }
        Err(format!(
            "Expected one visible enabled control {label:?}, found {}. Available: {}",
            matches.len(),
            self.widgets()
        ))
    }

    fn capture(&mut self, label: &str) -> Result<(), String> {
        self.step(label, true)?;
        // Named comparison checkpoints and failures retain reserved capacity
        // even when a long transition uses the intermediate-image allowance.
        let missing = self
            .report
            .steps
            .last()
            .is_some_and(|step| step.screenshot.is_none());
        if self.options.mode == RunMode::Visual && missing {
            if self.captures >= 160 {
                return Err("Named checkpoint capture exceeded the 160-image hard limit".into());
            }
            self.captures += 1;
            let name = format!("{}-{:03}.png", self.report.name, self.captures);
            self.save_frame(&name)?;
            let widgets = self.widgets();
            if let Some(step) = self.report.steps.last_mut() {
                step.screenshot = Some(name);
                step.semantic["widgets"] = widgets;
            }
        }
        if let Some(step) = self.report.steps.last_mut() {
            step.semantic["checkpoint"] = json!(label);
        }
        Ok(())
    }

    fn save_frame(&mut self, name: &str) -> Result<(), String> {
        use image::ImageEncoder as _;
        use image::codecs::png::{CompressionType, FilterType, PngEncoder};
        use std::io::Write as _;
        let started = Instant::now();
        let result = (|| {
            let frame = self.harness.render()?;
            let file =
                std::fs::File::create(self.options.output.join(name)).map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::new(file);
            PngEncoder::new_with_quality(&mut writer, CompressionType::Fast, FilterType::Sub)
                .write_image(
                    frame.as_raw(),
                    frame.width(),
                    frame.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| e.to_string())?;
            writer.flush().map_err(|e| e.to_string())
        })();
        self.capture_wall += started.elapsed();
        result
    }

    fn events(&mut self, label: &str, events: Vec<egui::Event>) -> Result<(), String> {
        self.last_input = Some(Instant::now());
        // RawInput preserves a whole native event batch. kittest's convenience
        // event queue otherwise splits each event into its own frame.
        self.harness.input_mut().events = events;
        self.step(label, true)
    }

    fn click(&mut self, label: &str) -> Result<(), String> {
        let rect = self.rect(label)?;
        self.click_at(label, rect.center())
    }

    fn click_at(&mut self, label: &str, pos: egui::Pos2) -> Result<(), String> {
        self.events(
            &format!("Pointer press: {label}"),
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )?;
        self.events(
            &format!("Pointer release: {label}"),
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        )
    }

    fn key(&mut self, key: egui::Key) -> Result<(), String> {
        self.key_modified(key, egui::Modifiers::NONE)
    }
    fn key_modified(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> Result<(), String> {
        self.events(
            &format!("Key {modifiers:?} {key:?}"),
            vec![
                key_event(key, modifiers, true),
                key_event(key, modifiers, false),
            ],
        )
    }
    fn command(&mut self, text: &str) -> Result<(), String> {
        self.key(egui::Key::Colon)?;
        self.events(
            &format!("Command {text}"),
            vec![
                egui::Event::Text(text.into()),
                key_event(egui::Key::Enter, egui::Modifiers::NONE, true),
                key_event(egui::Key::Enter, egui::Modifiers::NONE, false),
            ],
        )
    }
    fn chord(&mut self, keys: &[egui::Key]) -> Result<(), String> {
        for key in keys {
            self.key(*key)?;
        }
        Ok(())
    }

    fn wait_for(&mut self, label: &str, ready: impl Fn(&DeadpanApp) -> bool) -> Result<(), String> {
        let started = Instant::now();
        let capture_started = self.capture_wall;
        let mut count = 0;
        while !ready(self.app()) {
            if started
                .elapsed()
                .saturating_sub(self.capture_wall - capture_started)
                > Duration::from_secs(15)
            {
                self.metric(
                    label,
                    started.elapsed().as_secs_f64() * 1000.0,
                    SampleOutcome::TimedOut,
                );
                return Err(format!(
                    "Timed out waiting for {label}: {}",
                    self.snapshot()
                ));
            }
            self.step(label, count < 3 || count == 8 || count == 24)?;
            count += 1;
            if !ready(self.app()) {
                let elapsed = started
                    .elapsed()
                    .saturating_sub(self.capture_wall - capture_started);
                let remaining = Duration::from_secs(15).saturating_sub(elapsed);
                self.wake.wait_until(Instant::now() + remaining);
            }
        }
        self.metric(
            label,
            started.elapsed().as_secs_f64() * 1000.0,
            SampleOutcome::Completed,
        );
        Ok(())
    }

    fn settled(&mut self) -> Result<(), String> {
        self.wait_for("Project and picture settled", |app| {
            !app.service.is_busy()
                && !app.repeat_queue.active()
                && !app.presentation.loading()
                && !app.presentation.needs_render()
                && !app.importing()
        })?;
        let picture = self.app().presentation.diagnostic_snapshot();
        let app = self.app();
        let workspace = app
            .workspace
            .as_ref()
            .ok_or("Settled replay lost its workspace")?;
        let view = match app.view {
            View::Source => ProjectView::Source {
                asset: app.selected_source.clone().ok_or("No selected Original")?,
                frame: SourceFrameId(app.source_cursor.min(app.source_length().saturating_sub(1))),
            },
            View::Sequence => ProjectView::Sequence {
                frame: ProjectFrame(
                    i64::try_from(
                        app.sequence_cursor
                            .min(app.sequence_length().saturating_sub(1)),
                    )
                    .map_err(|e| e.to_string())?,
                ),
            },
        };
        let current = app.presentation.displayed_matches(
            workspace.session,
            workspace.document.project_id(),
            workspace.document.revision_id(),
            &view,
        );
        self.check(
            "Settled picture matches the current session, revision, context and cursor",
            picture["error"].is_null()
                && current
                && (picture["requested"].is_null() || picture["requested"] == picture["displayed"]),
            json!({"view":format!("{view:?}"),"revision":self.revision(),"error":null}),
            picture,
        )
    }
    fn revision(&self) -> String {
        self.app()
            .workspace
            .as_ref()
            .expect("fixture workspace")
            .document
            .revision_id()
            .as_str()
            .into()
    }
    fn changed(&mut self, before: &str) -> Result<(), String> {
        self.wait_for("Committed revision changed", |app| {
            app.workspace
                .as_ref()
                .is_some_and(|w| w.document.revision_id().as_str() != before)
        })?;
        self.settled()
    }
}

fn key_event(key: egui::Key, modifiers: egui::Modifiers, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    }
}
