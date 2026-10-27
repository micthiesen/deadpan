//! Latest-value cursor and completed-range display. Partial coverage never
//! enters the complete-analysis workspace or enables editorial text objects.

use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

use deadpan_cli::transcription::incremental::RangeProgress;

use super::*;

#[derive(Clone, Default)]
pub(super) struct RangeSender {
    pub cursor: Arc<AtomicU64>,
    pub pending: Arc<Mutex<Option<PreparedRange>>>,
    pub corrections: Arc<Mutex<Option<Arc<crate::project::OriginalCorrections>>>>,
}

pub(super) struct PreparedRange {
    pub progress: RangeProgress,
    pub words: Result<Arc<Transcript>, String>,
    word_ids: Vec<egui::Id>,
    correction_version: u64,
}

/// Assemble identity off the UI thread along with the corrected words. Earlier
/// completed ranges can prepend words and renumber segments; neither changes
/// the identity of a retained word. Equal tuples still get separate buttons.
fn partial_word_ids(words: &[deadpan_analysis::Word]) -> Vec<egui::Id> {
    let mut counts = std::collections::HashMap::new();
    words
        .iter()
        .map(|word| {
            let key = (word.start_cs, word.end_cs, word.text.as_str());
            let ordinal = counts.entry(key).or_insert(0_usize);
            let id = egui::Id::new(("partial-word", key, *ordinal));
            *ordinal += 1;
            id
        })
        .collect()
}

impl RangeSender {
    /// Correction application and full transcript copying stay on the worker.
    pub fn publish(&self, progress: RangeProgress) {
        let sheet = self
            .corrections
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let correction_version = sheet.as_ref().map_or(0, |sheet| sheet.version());
        let words = match sheet.as_ref() {
            Some(sheet) if sheet.error.is_some() => Err(sheet.error.clone().unwrap_or_default()),
            Some(sheet) => match sheet.stored.as_ref() {
                Some(stored) => stored
                    .corrections
                    .apply_to_transcript(&progress.transcript)
                    .map_err(|error| error.to_string())
                    .and_then(|corrected| {
                        if corrected.skipped > 0 {
                            Err("correction clock validation failed".into())
                        } else {
                            Ok(Arc::new(corrected.transcript))
                        }
                    }),
                None => Ok(Arc::new(progress.transcript.clone())),
            },
            None => Ok(Arc::new(progress.transcript.clone())),
        };
        let word_ids = words
            .as_ref()
            .map_or_else(|_| Vec::new(), |words| partial_word_ids(words.words()));
        *self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(PreparedRange {
            progress,
            words,
            word_ids,
            correction_version,
        });
    }
}

#[derive(Default)]
pub(super) struct RangeFeed {
    pub sender: RangeSender,
    pub shown: Option<Arc<PreparedRange>>,
}

impl RangeFeed {
    /// New attempts retain the current cursor/corrections but detach the old
    /// worker's publication slot, including any delayed post-failure update.
    pub fn restart(&mut self) {
        *self = Self {
            sender: RangeSender {
                cursor: Arc::clone(&self.sender.cursor),
                corrections: Arc::clone(&self.sender.corrections),
                ..RangeSender::default()
            },
            shown: None,
        };
    }

    pub fn receive(&mut self) {
        if let Ok(mut pending) = self.sender.pending.try_lock()
            && let Some(progress) = pending.take()
        {
            self.shown = Some(Arc::new(progress));
        }
    }
}

/// Convert exact source seconds to the complete Original's 16 kHz grid.
fn analysis_sample(seconds: deadpan_core::ExactRatio, origin: i64, rate: u32) -> Option<u64> {
    use deadpan_core::ExactRatio;
    let origin = ExactRatio::new(i128::from(origin), i128::from(rate)).ok()?;
    let position = seconds
        .checked_sub(origin)
        .ok()?
        .checked_mul(ExactRatio::integer(16_000))
        .ok()?
        .floor();
    u64::try_from(position.max(0)).ok()
}

fn covered_word(progress: &RangeProgress, word: &deadpan_analysis::Word) -> bool {
    if u64::from(word.start_cs) * 160 >= progress.total_samples {
        return false;
    }
    let midpoint =
        ((u64::from(word.start_cs) + u64::from(word.end_cs)) * 80).min(progress.total_samples - 1);
    progress
        .coverage
        .iter()
        .any(|range| range.contains(&midpoint))
}

impl DeadpanApp {
    pub(super) fn prioritize_analysis_cursor(&self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        *self
            .transcription
            .ranges
            .sender
            .corrections
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = workspace.corrections.clone();
        let Some(asset) = original_asset(workspace) else {
            return;
        };
        let Some(source) = workspace.sources.get(asset) else {
            return;
        };
        let snapshot = source.receipt.snapshot();
        let Some(audio) = snapshot.audio() else {
            return;
        };
        let Some(first) = audio.frames().first() else {
            return;
        };
        let seconds = match self.view {
            View::Source => snapshot.video().and_then(|video| {
                let index = video.index().index();
                let frame = self
                    .source_cursor
                    .min(index.frames().len().saturating_sub(1) as u64);
                picture_seconds(index, frame as usize)
            }),
            View::Sequence => {
                let frame = self
                    .sequence_cursor
                    .min(self.sequence_length().saturating_sub(1));
                workspace
                    .plan
                    .picture(ProjectFrame(frame as i64))
                    .ok()
                    .and_then(|sample| {
                        let (shown, point) = sample.picture.follow_point()?;
                        if shown != asset {
                            return None;
                        }
                        point
                            .ticks
                            .checked_mul(
                                deadpan_core::ExactRatio::new(
                                    i128::from(point.time_base.numerator()),
                                    i128::from(point.time_base.denominator()),
                                )
                                .ok()?,
                            )
                            .ok()
                    })
            }
        };
        if let Some(sample) = seconds.and_then(|seconds| {
            analysis_sample(seconds, first.valid_start, audio.stream().sample_rate)
        }) {
            self.transcription
                .ranges
                .sender
                .cursor
                .store(sample, Ordering::Release);
        }
    }

    /// Show real completed words before the entire Original finishes. Keep
    /// partial results separate from search, corrections and edit selectors.
    pub(super) fn partial_transcript_words(&mut self, ui: &mut egui::Ui) {
        let Some(prepared) = self.transcription.ranges.shown.clone() else {
            return;
        };
        let progress = &prepared.progress;
        ui.weak(format!(
            "Analyzed {:.1} / {:.1} s · incomplete Original",
            progress.completed_samples as f64 / 16_000.0,
            progress.total_samples as f64 / 16_000.0
        ));
        if progress.cache_failures > 0 {
            ui.weak("Some private range caches could not be reused or saved; analysis continues.");
        }
        let corrections = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.corrections.as_ref());
        // Loading can discover an unreadable sheet without changing its
        // numeric version. An old prepared range must not bypass that error.
        if let Some(error) = corrections.and_then(|sheet| sheet.error.as_deref()) {
            ui.colored_label(
                style::WARNING,
                format!("Completed words wait for corrections: {error}"),
            );
            return;
        }
        let version = corrections.map_or(0, |sheet| sheet.version());
        if version != prepared.correction_version {
            ui.weak("Completed words wait for the current corrections.");
            return;
        }
        let transcript = match &prepared.words {
            Ok(words) => words,
            Err(error) => {
                ui.colored_label(
                    style::WARNING,
                    format!("Completed words wait for corrections: {error}"),
                );
                return;
            }
        };
        let cursor_cs = self
            .transcription
            .ranges
            .sender
            .cursor
            .load(Ordering::Acquire)
            / 160;
        let words = transcript.words();
        let first = words
            .partition_point(|word| u64::from(word.start_cs) < cursor_cs)
            .saturating_sub(3);
        let end = (first + 9).min(words.len());
        let mut clicked = None;
        for (offset, word) in words[first..end].iter().enumerate() {
            if !covered_word(progress, word) {
                continue;
            }
            let label = if word.approximate() {
                format!("{} ≈", word.text)
            } else {
                word.text.clone()
            };
            // push_id still mixes the child UI's position into auto-widget
            // identities in egui. Explicitly seed this scope so a prepended
            // word cannot change the retained button's focus identity.
            let word_id = ui.make_persistent_id(prepared.word_ids[first + offset]);
            let response = ui
                .scope_builder(egui::UiBuilder::new().id(word_id), |ui| ui.button(label))
                .inner;
            // Source-pane focus can change the rail's height after activation.
            // Keep the focused word fully revealed through the retained paint.
            if response.has_focus() && !ui.clip_rect().contains_rect(response.rect) {
                ui.scroll_to_rect_animation(
                    response.rect.expand(2.0),
                    None,
                    egui::style::ScrollAnimation::none(),
                );
            }
            if response.clicked() {
                clicked = transcript.seconds(word.start_cs).ok();
            }
        }
        // Retain room below the last button so fractional scroll/pixel
        // rounding can reveal its complete border at the minimum height.
        ui.add_space(4.0);
        if let Some(seconds) = clicked {
            let target = self.workspace.as_ref().and_then(|workspace| {
                let asset = original_asset(workspace)?;
                let video = workspace.sources.get(asset)?.receipt.snapshot().video()?;
                let frame = picture_at(video.index().index(), seconds)?;
                Some((asset.clone(), frame as u64))
            });
            if let Some((asset, frame)) = target {
                self.select_source(asset);
                self.source_cursor = frame;
                self.moment.move_to(frame);
                self.request_picture(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_identity_survives_prepend_and_distinguishes_equal_observations() {
        let word = deadpan_analysis::Word {
            text: "again".into(),
            start_cs: 100,
            end_cs: 150,
            probability: 0.5,
            segment: 0,
        };
        let original = partial_word_ids(&[word.clone(), word.clone()]);
        let mut later = word.clone();
        later.segment = 2;
        let earlier = deadpan_analysis::Word {
            text: "earlier".into(),
            start_cs: 0,
            end_cs: 50,
            ..word
        };
        let prepended = partial_word_ids(&[earlier, later.clone(), later]);
        assert_eq!(original, prepended[1..]);
        assert_ne!(original[0], original[1]);
    }

    #[test]
    fn priority_uses_absolute_pts_audio_origin_and_exact_fractional_rate() {
        use deadpan_core::ExactRatio;
        assert_eq!(
            analysis_sample(ExactRatio::new(1, 2).unwrap(), -4_410, 44_100),
            Some(9_600)
        );
        assert_eq!(
            analysis_sample(ExactRatio::new(-1, 2).unwrap(), -4_410, 44_100),
            Some(0)
        );
        assert_eq!(
            analysis_sample(ExactRatio::new(1001, 30_000).unwrap(), 0, 44_100),
            Some(533)
        );
    }

    #[test]
    fn completion_slot_replaces_obsolete_ui_updates() {
        let mut feed = RangeFeed::default();
        let progress = |done| RangeProgress {
            transcript: Transcript::new(
                deadpan_analysis::AnalysedAudio {
                    origin: 0,
                    sample_rate: 48_000,
                    duration_cs: 4_800,
                },
                vec![],
            )
            .unwrap(),
            coverage: std::iter::once(384_000..768_000).collect(),
            total_samples: 768_000,
            completed_samples: done,
            cached_ranges: 0,
            cache_failures: 0,
        };
        feed.sender.publish(progress(384_000));
        feed.sender.publish(progress(768_000));
        feed.receive();
        assert_eq!(
            feed.shown.as_ref().unwrap().progress.completed_samples,
            768_000
        );
        assert!(feed.sender.pending.lock().unwrap().is_none());
        let old_worker = feed.sender.clone();
        old_worker.cursor.store(533, Ordering::Release);
        feed.restart();
        old_worker.publish(progress(384_000));
        feed.receive();
        assert!(
            feed.shown.is_none(),
            "a previous attempt cannot publish into a retry"
        );
        assert_eq!(feed.sender.cursor.load(Ordering::Acquire), 533);
        feed.sender.publish(progress(384_000));
        feed.receive();
        assert_eq!(
            feed.shown.as_ref().unwrap().progress.completed_samples,
            384_000
        );
    }

    #[test]
    fn partial_rail_keeps_the_rounded_terminal_word_only_with_measured_support() {
        let progress = RangeProgress {
            transcript: Transcript::new(
                deadpan_analysis::AnalysedAudio {
                    origin: 0,
                    sample_rate: 44_100,
                    duration_cs: 7,
                },
                vec![],
            )
            .unwrap(),
            coverage: std::iter::once(0..1_001).collect(),
            total_samples: 1_001,
            completed_samples: 1_001,
            cached_ranges: 0,
            cache_failures: 0,
        };
        let word = deadpan_analysis::Word {
            text: "ending".into(),
            start_cs: 6,
            end_cs: 7,
            probability: 0.5,
            segment: 0,
        };
        assert!(covered_word(&progress, &word));
        assert!(!covered_word(
            &progress,
            &deadpan_analysis::Word {
                start_cs: 7,
                ..word
            }
        ));
    }
}
