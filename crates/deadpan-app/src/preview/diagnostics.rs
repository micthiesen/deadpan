//! The Diagnostics panel: live, process-local performance counters
//! (specification Section 25.3) beside the picture.
//!
//! `:diagnostics` opens it; Escape or Close closes it. It never pauses
//! playback, so delivery, queue and GPU counters keep moving while it is
//! open. Values are sampled twice a second, not every frame, and each row is
//! exposed to assistive technology as "label: value". Nothing shown here is
//! authored state, history or project data.

use std::time::{Duration, Instant};

use deadpan_diagnostics::{CacheSnapshot, IoSnapshot, Level, WorkerMemorySnapshot};

use super::*;

/// Sampling period while the panel is open.
pub(super) const REFRESH: Duration = Duration::from_millis(500);

/// One sampled moment of every counter the app process can observe.
#[derive(Clone, Copy)]
pub(super) struct Sample {
    pub(super) at: Instant,
    pub(super) counters: deadpan_diagnostics::Snapshot,
    pub(super) playback: deadpan_playback::Diagnostics,
    /// Outer UI updates since the app started.
    pub(super) frames: u64,
}

/// One formatted row: section, label, value and its accessible text.
pub(super) type Row = (&'static str, &'static str, String, String);

#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    focus_pending: bool,
    return_focus: Option<(u64, Pane)>,
    /// Outer UI frames, counted once per frame (never per layout retry).
    frames: u64,
    pub(super) latest: Option<Sample>,
    previous: Option<Sample>,
    /// Samples taken since the panel opened.
    pub(super) refreshes: u64,
    /// Rows formatted once per sample, not on every painted frame.
    formatted: Vec<Row>,
}

impl State {
    pub(super) fn count_frame(&mut self) {
        self.frames += 1;
    }
}

fn binary_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn level(value: Level) -> String {
    format!("{} now · {} high", value.current, value.high)
}

fn io(value: IoSnapshot) -> String {
    if value == IoSnapshot::default() {
        return "none yet".into();
    }
    format!(
        "read {} in {} · wrote {} in {}",
        binary_bytes(value.read_bytes),
        value.read_ops,
        binary_bytes(value.write_bytes),
        value.write_ops
    )
}

fn cache(value: CacheSnapshot, bytes: bool) -> String {
    let rate = value.hit_rate().map_or_else(
        || "no lookups".to_owned(),
        |rate| format!("{:.0}% hits", rate * 100.0),
    );
    let mut text = format!(
        "{rate} ({} / {}) · {} evicted · {} resident ({} high)",
        value.hits,
        value.hits + value.misses,
        value.evictions,
        value.entries.current,
        value.entries.high
    );
    if bytes {
        text.push_str(&format!(
            " · {} ({} high)",
            binary_bytes(value.bytes.current),
            binary_bytes(value.bytes.high)
        ));
    }
    text
}

fn workers(value: WorkerMemorySnapshot) -> String {
    if value.live.high == 0 {
        return "none has run in this session".into();
    }
    let mut text = format!(
        "{} live · {} now · {} peak",
        value.live.current,
        binary_bytes(value.footprint_bytes.current),
        binary_bytes(value.footprint_bytes.high)
    );
    if value.failures != 0 {
        text.push_str(&format!(" · {} samples refused", value.failures));
    }
    text
}

fn micros(value: u64) -> String {
    if value >= 1000 {
        format!("{:.1} ms", value as f64 / 1000.0)
    } else {
        format!("{value} µs")
    }
}

/// The panel's rows as (section, label, value).
pub(super) fn rows(
    sample: &Sample,
    previous: Option<&Sample>,
) -> Vec<(&'static str, &'static str, String)> {
    let counters = &sample.counters;
    let playback = &sample.playback;
    let gpu = counters.gpu;
    let mut rows = vec![
        (
            "PLAYBACK",
            "Underruns",
            format!(
                "{} starved of {} device reports",
                playback.starved, playback.reports
            ),
        ),
        ("PLAYBACK", "Device faults", playback.faults.to_string()),
        (
            "PLAYBACK",
            "Silent padding",
            format!("{} frames", playback.silent_frames),
        ),
        (
            "PLAYBACK",
            "Callback render cost",
            format!("{} max", micros(playback.max_render_cost_ns / 1000)),
        ),
        ("PLAYBACK", "Generations", playback.generations.to_string()),
        (
            "DECODE QUEUES",
            "Picture preview",
            level(counters.queues.picture_preview),
        ),
        (
            "DECODE QUEUES",
            "Card thumbnails",
            level(counters.queues.thumbnails),
        ),
        (
            "DECODE QUEUES",
            "Prepared audio",
            level(counters.queues.playback_prepared),
        ),
        (
            "DECODE QUEUES",
            "Device packets",
            level(counters.queues.playback_device_packets),
        ),
        (
            "DECODE QUEUES",
            "Last prefill",
            format!("{} frames", counters.queues.playback_prefill_frames.current),
        ),
        (
            "GPU",
            "Submissions",
            format!(
                "{} submitted · {} completed",
                gpu.submissions, gpu.completions
            ),
        ),
        (
            "GPU",
            "Completion latency",
            match (gpu.p50_us, gpu.p95_us) {
                (Some(p50), Some(p95)) => format!(
                    "last {} · p50 {} · p95 {} · max {}",
                    micros(gpu.last_us),
                    micros(p50),
                    micros(p95),
                    micros(gpu.max_us)
                ),
                _ => "no completed submission".into(),
            },
        ),
        (
            "PCM CACHES",
            "Decoded sources",
            cache(counters.caches.decoded_pcm, true),
        ),
        (
            "PCM CACHES",
            "Limiter tiles",
            cache(counters.caches.limiter_tiles, false),
        ),
        (
            "PCM CACHES",
            "Limiter inputs",
            cache(counters.caches.limiter_inputs, true),
        ),
    ];
    for (name, label) in [
        // Each row names exactly what it counts; other file I/O is not
        // counted (see docs/PERFORMANCE.md#diagnostics).
        ("store_revisions", "Store history rows"),
        ("objects", "Media objects"),
        ("media_snapshots", "Media snapshots"),
        ("decoder_input", "Video decoder reads"),
        ("pcm_cache_file", "PCM cache file"),
        ("proxy", "Proxy hashing reads"),
    ] {
        let value = counters
            .io
            .paths()
            .into_iter()
            .find(|(path, _)| *path == name)
            .map(|(_, value)| value)
            .unwrap_or_default();
        rows.push(("FILE I/O", label, io(value)));
    }
    rows.push((
        "MODEL MEMORY",
        "Model workers",
        workers(counters.model_workers),
    ));
    rows.push((
        "MODEL MEMORY",
        "Other workers",
        workers(counters.other_workers),
    ));
    let rate = previous
        .map(|previous| {
            let seconds = sample
                .at
                .saturating_duration_since(previous.at)
                .as_secs_f64();
            if seconds > 0.0 {
                format!(
                    " · {:.1}/s",
                    (sample.frames - previous.frames) as f64 / seconds
                )
            } else {
                String::new()
            }
        })
        .unwrap_or_default();
    // Every outer update counts, including those this panel's own refresh
    // requests, so the rate while open is not an idle measurement.
    rows.push((
        "UI",
        "UI updates",
        format!("{} since start{rate} (includes this panel)", sample.frames),
    ));
    rows
}

/// [`rows`] with each row's accessible "label: value" text.
fn formatted(sample: &Sample, previous: Option<&Sample>) -> Vec<Row> {
    rows(sample, previous)
        .into_iter()
        .map(|(section, label, value)| {
            let spoken = format!("{label}: {value}");
            (section, label, value, spoken)
        })
        .collect()
}

impl DeadpanApp {
    pub(super) fn open_diagnostics(&mut self, context: &egui::Context) {
        self.bindings.clear();
        self.diagnostics.open = true;
        self.diagnostics.focus_pending = true;
        self.diagnostics.refreshes = 0;
        self.diagnostics.previous = None;
        self.diagnostics.latest = None;
        self.sample_diagnostics();
        context.request_repaint();
    }

    fn close_diagnostics(&mut self, context: &egui::Context) {
        self.diagnostics.open = false;
        self.diagnostics.focus_pending = false;
        self.diagnostics.return_focus = Some((context.cumulative_frame_nr(), self.pane));
        context.request_discard("diagnostics closed");
        context.request_repaint();
    }

    fn sample_diagnostics(&mut self) {
        let sample = Sample {
            at: Instant::now(),
            counters: deadpan_diagnostics::snapshot(),
            playback: self.playback.diagnostics(),
            frames: self.diagnostics.frames,
        };
        self.diagnostics.previous = self.diagnostics.latest.replace(sample);
        self.diagnostics.formatted = formatted(&sample, self.diagnostics.previous.as_ref());
        self.diagnostics.refreshes += 1;
    }

    /// Count the outer frame and, while open, resample at [`REFRESH`].
    /// Call once per outer frame, never on a layout retry.
    pub(super) fn reconcile_diagnostics(&mut self, context: &egui::Context) {
        self.diagnostics.count_frame();
        if !self.diagnostics.open {
            return;
        }
        let due = self.diagnostics.latest.map_or(Duration::ZERO, |sample| {
            REFRESH.saturating_sub(sample.at.elapsed())
        });
        if due.is_zero() {
            self.sample_diagnostics();
            context.request_repaint_after(REFRESH);
        } else {
            context.request_repaint_after(due);
        }
    }

    /// The panel owns the keyboard while open: editor bindings never see keys,
    /// Tab and Space/Enter stay native, and Escape closes it.
    pub(super) fn diagnostics_keyboard(&mut self, context: &egui::Context) -> bool {
        if let Some((frame, pane)) = self.diagnostics.return_focus
            && context.cumulative_frame_nr() > frame
        {
            context.memory_mut(|memory| memory.request_focus(pane_id(pane)));
            self.diagnostics.return_focus = None;
        }
        if !self.diagnostics.open {
            return false;
        }
        let composing = &mut self.ime_composing;
        context.input(|input| help_scroll::observe_composition(&input.events, composing));
        self.bindings.clear();
        true
    }

    pub(super) fn diagnostics_window(&mut self, context: &egui::Context) {
        if !self.diagnostics.open {
            return;
        }
        if self.diagnostics.latest.is_none() {
            return;
        }
        // Borrow the rows formatted at the last sample; restored below.
        let rows = std::mem::take(&mut self.diagnostics.formatted);
        let content = context.content_rect();
        let width = (content.width() - 32.0).clamp(280.0, 380.0);
        // Header, sheet chrome and the footer stay outside the scroller, so
        // the sheet ends above the window's bottom edge.
        let height = (content.height() - 250.0).max(160.0);
        let mut close = false;
        // Anchored beside the picture with no dimming: the picture stays
        // dominant and playback continues underneath.
        let modal = egui::Modal::new(egui::Id::new("diagnostics-window"))
            .backdrop_color(egui::Color32::TRANSPARENT)
            .area(
                egui::Modal::default_area(egui::Id::new("diagnostics-window"))
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 56.0)),
            )
            .show(context, |ui| {
                accessibility::dialog(ui, "Diagnostics");
                ui.set_width(width);
                ui.label(style::section_title("DIAGNOSTICS", true));
                ui.label(
                    egui::RichText::new(
                        "Live counters for this app process only; file I/O rows cover the named paths, not all I/O. They are never saved with the project.",
                    )
                    .size(11.5)
                    .weak(),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("diagnostics-rows")
                    .max_height(height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let mut start = 0;
                        while start < rows.len() {
                            let heading = rows[start].0;
                            let end = rows[start..]
                                .iter()
                                .position(|row| row.0 != heading)
                                .map_or(rows.len(), |offset| start + offset);
                            if start != 0 {
                                ui.add_space(6.0);
                            }
                            ui.label(style::section_title(heading, false));
                            egui::Grid::new(("diagnostics-section", heading))
                                .num_columns(2)
                                .min_col_width(118.0)
                                .spacing(egui::vec2(10.0, 3.0))
                                .min_row_height(16.0)
                                .show(ui, |ui| {
                                    for (_, label, value, spoken) in &rows[start..end] {
                                        ui.label(egui::RichText::new(*label).size(12.0).weak());
                                        let response = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(value).monospace().size(11.5),
                                            )
                                            .wrap(),
                                        );
                                        accessibility::full_text(response, spoken);
                                        ui.end_row();
                                    }
                                });
                            start = end;
                        }
                    });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let button = ui.add(style::action("Close", "Esc"));
                    if std::mem::take(&mut self.diagnostics.focus_pending) {
                        button.request_focus();
                    }
                    if button.clicked() {
                        close = true;
                    }
                    ui.label(
                        egui::RichText::new("Updates twice a second")
                            .size(11.0)
                            .weak(),
                    );
                });
            });
        self.diagnostics.formatted = rows;
        close |= modal.should_close() && !self.ime_composing;
        if close {
            self.close_diagnostics(context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_read_as_units_and_rates() {
        assert_eq!(binary_bytes(512), "512 B");
        assert_eq!(binary_bytes(1536), "1.5 KiB");
        assert_eq!(binary_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
        assert_eq!(micros(850), "850 µs");
        assert_eq!(micros(2_500), "2.5 ms");
        assert_eq!(
            level(Level {
                current: 1,
                high: 2
            }),
            "1 now · 2 high"
        );
        assert_eq!(io(IoSnapshot::default()), "none yet");
        assert_eq!(
            io(IoSnapshot {
                read_ops: 2,
                read_bytes: 2048,
                write_ops: 1,
                write_bytes: 10
            }),
            "read 2.0 KiB in 2 · wrote 10 B in 1"
        );
        assert_eq!(
            workers(WorkerMemorySnapshot::default()),
            "none has run in this session"
        );
        let sample = Sample {
            at: Instant::now(),
            counters: deadpan_diagnostics::Snapshot::default(),
            playback: deadpan_playback::Diagnostics::default(),
            frames: 30,
        };
        let earlier = Sample {
            at: sample.at - Duration::from_secs(1),
            frames: 28,
            ..sample
        };
        let rows = rows(&sample, Some(&earlier));
        assert!(rows.iter().any(|(_, label, value)| *label == "UI updates"
            && value == "30 since start · 2.0/s (includes this panel)"));
        assert!(rows.iter().any(
            |(_, label, value)| *label == "Decoded sources" && value.starts_with("no lookups")
        ));
        assert!(
            rows.iter()
                .any(|(_, label, value)| *label == "Completion latency"
                    && value == "no completed submission")
        );
    }
}
