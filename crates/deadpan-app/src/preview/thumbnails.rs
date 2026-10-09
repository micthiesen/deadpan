//! Card thumbnails for the Original and the visible beats.
//!
//! A dedicated preview worker decodes the first picture of each visible card
//! from the committed workspace. Replies render through the shared SDR
//! pipeline into small retained textures. Only visible cards request work and
//! the cache is bounded, so a long edit never allocates one texture per beat.
//! A card keeps its previous texture until the current revision's replacement
//! renders, so edits never flash empty thumbnails.

use std::collections::BTreeMap;

use super::*;

/// Retained textures. Larger than any visible strip, small enough that a long
/// project cannot grow GPU memory without bound.
const MAX_ENTRIES: usize = 48;

/// Which card a thumbnail belongs to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Slot {
    Original,
    Beat(NodeId),
    /// A Ready AI variant, keyed by its attempt. Its picture is immutable, so
    /// its key's revision is the request's origin and edits never re-decode it.
    Candidate(deadpan_jobs::AttemptId),
}

/// The exact committed picture a thumbnail shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Key {
    pub session: u64,
    pub revision: RevisionId,
    pub slot: Slot,
    pub view: ProjectView,
}

struct Entry<T> {
    key: Key,
    value: T,
    used: u64,
}

/// Bounded least-recently-used storage keyed by card. Pure, so its request and
/// eviction policy is testable without a GPU.
struct Cache<T> {
    entries: BTreeMap<Slot, Entry<T>>,
    clock: u64,
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            clock: 0,
        }
    }
}

impl<T> Cache<T> {
    /// The retained value for a card, whether it matches the wanted key, and
    /// the session that produced it.
    fn get(&mut self, key: &Key) -> Option<(&T, bool, u64)> {
        self.clock += 1;
        let clock = self.clock;
        self.entries.get_mut(&key.slot).map(|entry| {
            entry.used = clock;
            (&entry.value, entry.key == *key, entry.key.session)
        })
    }

    /// The first wanted key without a current value, in display order.
    fn missing<'a>(&self, wanted: &'a [Key]) -> Option<&'a Key> {
        wanted.iter().find(|key| {
            self.entries
                .get(&key.slot)
                .is_none_or(|entry| entry.key != **key)
        })
    }

    /// Store a value and return every value it displaced or evicted.
    fn insert(&mut self, key: Key, value: T) -> Vec<T> {
        self.clock += 1;
        let mut removed = Vec::new();
        if let Some(old) = self.entries.insert(
            key.slot.clone(),
            Entry {
                key,
                value,
                used: self.clock,
            },
        ) {
            removed.push(old.value);
        }
        while self.entries.len() > MAX_ENTRIES {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(slot, _)| slot.clone())
                .expect("nonempty cache");
            removed.extend(self.entries.remove(&oldest).map(|entry| entry.value));
        }
        removed
    }

    fn clear(&mut self) -> Vec<T> {
        std::mem::take(&mut self.entries)
            .into_values()
            .map(|entry| entry.value)
            .collect()
    }
}

/// A rendered thumbnail, or an explicit absence that must not be requested
/// again for the same key: an authored Background or a failed decode.
pub(super) struct Thumb {
    picture: Option<RegisteredTarget>,
    aspect: f32,
}

/// What a card paints: the texture and its aspect, or a black Background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Painted {
    Texture {
        texture: egui::TextureId,
        aspect: f32,
    },
    Black {
        aspect: f32,
    },
}

pub(super) struct Thumbnails {
    worker: PreviewWorker,
    state: egui_wgpu::RenderState,
    cache: Cache<Thumb>,
    session: Option<u64>,
    serial: u64,
    pending: Option<(u64, Key)>,
    received: Option<(Key, Result<crate::worker::Picture, String>)>,
    wanted: Vec<Key>,
    /// What each wanted variant thumbnail decodes, by attempt.
    candidates: BTreeMap<deadpan_jobs::AttemptId, Arc<crate::worker::CandidateThumbnail>>,
    /// Displaced textures may still be referenced by the frame being painted;
    /// they are freed when the next frame begins.
    retired: Vec<Thumb>,
}

impl Thumbnails {
    pub fn new(context: egui::Context, state: egui_wgpu::RenderState) -> std::io::Result<Self> {
        Ok(Self {
            worker: PreviewWorker::named(context, "deadpan-thumbnails")?,
            state,
            cache: Cache::default(),
            session: None,
            serial: 0,
            pending: None,
            received: None,
            wanted: Vec::new(),
            candidates: BTreeMap::new(),
            retired: Vec::new(),
        })
    }

    /// Start a layout pass. A different project session drops every texture.
    pub fn begin(&mut self, session: Option<u64>) {
        for thumb in std::mem::take(&mut self.retired) {
            self.release(thumb);
        }
        if session != self.session {
            self.session = session;
            self.release_all();
        }
        self.wanted.clear();
        self.candidates.clear();
    }

    /// Record a visible AI variant and return what it should paint now.
    pub fn show_candidate(
        &mut self,
        key: Key,
        thumbnail: Arc<crate::worker::CandidateThumbnail>,
    ) -> Option<Painted> {
        if let Slot::Candidate(attempt) = &key.slot {
            self.candidates.insert(attempt.clone(), thumbnail);
        }
        self.show(key)
    }

    /// Record a visible card and return what it should paint now.
    pub fn show(&mut self, key: Key) -> Option<Painted> {
        let session = key.session;
        let painted = self.cache.get(&key).and_then(|(thumb, _, cached)| {
            // A previous project's picture never stands in for this one.
            if cached != session {
                return None;
            }
            Some(match &thumb.picture {
                Some(target) => Painted::Texture {
                    texture: target.texture,
                    aspect: thumb.aspect,
                },
                None if thumb.aspect > 0.0 => Painted::Black {
                    aspect: thumb.aspect,
                },
                None => return None,
            })
        });
        if !self.wanted.contains(&key) {
            self.wanted.push(key);
        }
        painted
    }

    /// Receive one reply, render it when the shared renderer is free, then
    /// request the next missing visible thumbnail. The main picture has
    /// priority: nothing renders while it waits for a submission.
    pub fn pump(
        &mut self,
        context: &egui::Context,
        workspace: Option<&Arc<Workspace>>,
        renderer: &mut PictureRenderer,
        main_busy: bool,
    ) {
        if let Some(reply) = self.worker.take_reply()
            && let Some((request, key)) = self.pending.take()
        {
            if reply.ticket.request == request {
                self.received = Some((key, reply.picture));
            } else {
                self.pending = Some((request, key));
            }
        }
        if let Some((key, _)) = &self.received
            && !self.wanted.contains(key)
        {
            // Scrolled away or superseded by an edit; never render it.
            self.received = None;
        }
        if self.received.is_some() && !main_busy {
            match renderer.is_idle() {
                Ok(true) => {
                    let (key, picture) = self.received.take().expect("received thumbnail");
                    let thumb = self.render(context, renderer, picture);
                    self.retired.extend(self.cache.insert(key, thumb));
                    // The cards already painted this frame's previous state.
                    context.request_repaint();
                }
                Ok(false) => context.request_repaint_after(Duration::from_millis(16)),
                Err(_) => {
                    // Record a failure for this key instead of stalling.
                    let (key, _) = self.received.take().expect("received thumbnail");
                    self.retired.extend(self.cache.insert(
                        key,
                        Thumb {
                            picture: None,
                            aspect: 0.0,
                        },
                    ));
                }
            }
        } else if self.received.is_some() {
            context.request_repaint_after(Duration::from_millis(16));
        }
        if self.pending.is_some() || self.received.is_some() {
            return;
        }
        let Some(workspace) = workspace else {
            return;
        };
        let Some(key) = self.cache.missing(&self.wanted).cloned() else {
            return;
        };
        let work = match &key.slot {
            Slot::Candidate(attempt) => {
                let Some(thumbnail) = self.candidates.get(attempt) else {
                    return;
                };
                if key.session != workspace.session {
                    return;
                }
                Work::CandidateThumbnail {
                    workspace: Arc::clone(workspace),
                    thumbnail: Arc::clone(thumbnail),
                }
            }
            Slot::Original | Slot::Beat(_) => {
                if key.session != workspace.session
                    || &key.revision != workspace.document.revision_id()
                {
                    return;
                }
                Work::Project {
                    workspace: Arc::clone(workspace),
                    view: key.view.clone(),
                }
            }
        };
        self.serial += 1;
        let ticket = Ticket {
            transport: None,
            source: key.session,
            request: self.serial,
        };
        self.worker.submit(ticket, work);
        self.pending = Some((self.serial, key));
    }

    fn render(
        &self,
        context: &egui::Context,
        renderer: &mut PictureRenderer,
        picture: Result<crate::worker::Picture, String>,
    ) -> Thumb {
        let failed = Thumb {
            picture: None,
            aspect: 0.0,
        };
        let Ok(picture) = picture else {
            return failed;
        };
        let aspect = match (picture.canvas, picture.frame.as_ref()) {
            (Some((width, height)), _) => width as f32 / height as f32,
            (None, Some(frame)) => display_aspect(frame.metadata()),
            (None, None) => 16.0 / 9.0,
        };
        let Some(frame) = picture.frame.as_ref() else {
            // An authored Background is a successful black picture.
            return Thumb {
                picture: None,
                aspect,
            };
        };
        let raster = target_size(
            egui::vec2(cards::THUMBNAIL_HEIGHT * aspect, cards::THUMBNAIL_HEIGHT),
            context.pixels_per_point(),
        );
        let Ok(target) = renderer.create_target(raster.0, raster.1) else {
            return failed;
        };
        let result = if let Some((width, height)) = picture.canvas {
            match camera::render_layers(&picture) {
                Ok(layers) => renderer.render_composed(
                    frame,
                    &target,
                    picture.picture_context.as_deref(),
                    [width, height],
                    FitMode::Fit,
                    &layers,
                ),
                Err(_) => return failed,
            }
        } else {
            renderer.render(frame, &target, FitMode::Fit)
        };
        if result.is_err() {
            return failed;
        }
        let texture = self.state.renderer.write().register_native_texture(
            &self.state.device,
            target.display_view(),
            eframe::wgpu::FilterMode::Linear,
        );
        Thumb {
            picture: Some(RegisteredTarget { target, texture }),
            aspect,
        }
    }

    fn release(&self, thumb: Thumb) {
        if let Some(registered) = thumb.picture {
            self.state
                .renderer
                .write()
                .free_texture(&registered.texture);
        }
    }

    /// Media became available again (a relinked Original): drop remembered
    /// failures and textures so visible cards decode afresh.
    pub fn reset(&mut self) {
        self.release_all();
    }

    fn release_all(&mut self) {
        self.worker.cancel();
        self.pending = None;
        self.received = None;
        let cleared = self.cache.clear();
        self.retired.extend(cleared);
    }

    /// Rendered textures whose key matches the workspace's current revision.
    #[cfg(feature = "ui-harness")]
    pub fn rendered_for_revision(&self, workspace: Option<&Workspace>) -> usize {
        workspace.map_or(0, |workspace| {
            self.cache
                .entries
                .values()
                .filter(|entry| {
                    entry.value.picture.is_some()
                        && entry.key.session == workspace.session
                        && &entry.key.revision == workspace.document.revision_id()
                })
                .count()
        })
    }

    /// Rendered variant thumbnails of the current session.
    #[cfg(feature = "ui-harness")]
    pub fn rendered_candidates(&self, session: u64) -> usize {
        self.cache
            .entries
            .iter()
            .filter(|(slot, entry)| {
                matches!(slot, Slot::Candidate(_))
                    && entry.value.picture.is_some()
                    && entry.key.session == session
            })
            .count()
    }

    #[cfg(feature = "ui-harness")]
    pub fn retained(&self) -> usize {
        self.cache.entries.len()
    }

    pub fn shutdown(&mut self) {
        self.release_all();
        for thumb in std::mem::take(&mut self.retired) {
            self.release(thumb);
        }
        self.worker.shutdown();
    }
}

/// Displayed proportions of an uncomposed source frame: pixel aspect first,
/// then the quarter-turn rotations that swap width and height.
fn display_aspect(metadata: &deadpan_render::FrameMetadata) -> f32 {
    let [width, height] = metadata.clean_aperture.map_or(
        [f64::from(metadata.width), f64::from(metadata.height)],
        |aperture| {
            let rect = aperture.rect();
            [rect[2], rect[3]].map(|v| v.numerator() as f64 / v.denominator() as f64)
        },
    );
    let aspect = width * metadata.sample_aspect_ratio.as_f64() / height;
    match metadata.rotation {
        deadpan_render::Rotation::Clockwise90 | deadpan_render::Rotation::Clockwise270 => {
            (1.0 / aspect) as f32
        }
        deadpan_render::Rotation::None | deadpan_render::Rotation::Clockwise180 => aspect as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_aspect_uses_fractional_clean_extents_before_sar_and_rotation() {
        use deadpan_core::{ExactRatio, SourceTimeBase, SourceTimestamp};
        use deadpan_render::{
            CleanAperture, FrameMetadata, Rotation, SampleAspectRatio, SourceColor,
        };
        let metadata = FrameMetadata {
            width: 10,
            height: 8,
            row_stride_bytes: 40,
            clean_aperture: Some(
                CleanAperture::new(
                    [(1, 2), (1, 2), (17, 2), (13, 2)].map(|(n, d)| ExactRatio::new(n, d).unwrap()),
                )
                .unwrap(),
            ),
            sample_aspect_ratio: SampleAspectRatio::new(2, 1).unwrap(),
            rotation: Rotation::Clockwise90,
            color: SourceColor {
                transfer: deadpan_render::Transfer::Srgb,
                primaries: deadpan_render::Primaries::Rec709,
            },
            pts: SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 24).unwrap(),
            },
        };
        assert!((display_aspect(&metadata) - 13.0 / 34.0).abs() < 1e-6);
        assert!(
            (display_aspect(&FrameMetadata {
                rotation: Rotation::None,
                ..metadata
            }) - 34.0 / 13.0)
                .abs()
                < 1e-6
        );
    }

    fn key(slot: Slot, revision: &str, frame: i64) -> Key {
        Key {
            session: 1,
            revision: RevisionId::new(revision).unwrap(),
            slot,
            view: ProjectView::Sequence {
                frame: ProjectFrame(frame),
            },
        }
    }

    fn beat(name: &str) -> Slot {
        Slot::Beat(NodeId::new(name).unwrap())
    }

    #[test]
    fn missing_follows_display_order_and_retains_stale_values_until_replaced() {
        let mut cache = Cache::default();
        let first = key(beat("a"), "r1", 0);
        let second = key(beat("b"), "r1", 10);
        assert_eq!(
            cache.missing(&[first.clone(), second.clone()]),
            Some(&first)
        );
        assert!(cache.insert(first.clone(), 1).is_empty());
        assert_eq!(
            cache.missing(&[first.clone(), second.clone()]),
            Some(&second)
        );
        assert!(cache.insert(second.clone(), 2).is_empty());
        assert_eq!(cache.missing(&[first.clone(), second.clone()]), None);

        // A new revision wants fresh pictures, but the old ones still paint.
        let edited = key(beat("a"), "r2", 0);
        assert_eq!(cache.missing(std::slice::from_ref(&edited)), Some(&edited));
        assert_eq!(cache.get(&edited), Some((&1, false, 1)));
        assert_eq!(cache.insert(edited.clone(), 3), vec![1]);
        assert_eq!(cache.get(&edited), Some((&3, true, 1)));
    }

    #[test]
    fn eviction_is_bounded_and_drops_the_least_recently_shown_card() {
        let mut cache = Cache::default();
        let keys: Vec<_> = (0..MAX_ENTRIES)
            .map(|index| key(beat(&format!("beat-{index}")), "r1", index as i64))
            .collect();
        for (index, key) in keys.iter().enumerate() {
            assert!(cache.insert(key.clone(), index).is_empty());
        }
        // Showing the first card makes the second the oldest.
        assert!(cache.get(&keys[0]).is_some());
        let extra = key(Slot::Original, "r1", 0);
        assert_eq!(cache.insert(extra.clone(), usize::MAX), vec![1]);
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        assert!(cache.get(&keys[1]).is_none());
        assert!(cache.get(&keys[0]).is_some());
        assert_eq!(cache.clear().len(), MAX_ENTRIES);
        assert!(cache.missing(&[extra]).is_some());
    }
}
