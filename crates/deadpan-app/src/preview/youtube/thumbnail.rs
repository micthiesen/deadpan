use super::*;
use deadpan_cli::youtube::thumbnail::{Thumbnail, ThumbnailState};

pub(super) struct Cached {
    image: Arc<Thumbnail>,
    texture: egui::TextureHandle,
}

impl Flow {
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn thumbnail_texture(&self) -> Option<egui::TextureId> {
        self.thumbnail.as_ref().map(|cache| cache.texture.id())
    }

    pub(super) fn confirm_thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        state: &ThumbnailState,
        title: &str,
    ) {
        let ThumbnailState::Ready(image) = state else {
            self.thumbnail = None;
            match state {
                ThumbnailState::Missing => {
                    ui.weak("No thumbnail provided");
                }
                ThumbnailState::Unavailable(reason) => {
                    ui.weak("Thumbnail unavailable")
                        .on_hover_text(reason.description());
                }
                ThumbnailState::Ready(_) => unreachable!(),
            }
            return;
        };
        if self
            .thumbnail
            .as_ref()
            .is_none_or(|cache| !Arc::ptr_eq(&cache.image, image))
        {
            let [width, height] = image
                .dimensions()
                .map(|n| usize::try_from(n).expect("bounded thumbnail dimensions"));
            self.thumbnail = Some(Cached {
                image: image.clone(),
                texture: ui.ctx().load_texture(
                    "YouTube confirmation thumbnail",
                    egui::ColorImage::from_rgba_unmultiplied([width, height], image.rgba()),
                    egui::TextureOptions::LINEAR,
                ),
            });
        }
        if let Some(cache) = &self.thumbnail {
            let [width, height] = image.dimensions();
            let raster = egui::vec2(width as f32, height as f32);
            let max_height = if ui.ctx().content_rect().height() < 720.0 {
                96.0
            } else {
                160.0
            };
            let scale = (ui.available_width() / raster.x)
                .min(max_height / raster.y)
                .min(1.0);
            ui.add(
                egui::Image::new((cache.texture.id(), raster * scale))
                    .alt_text(format!("YouTube thumbnail for {title}"))
                    .corner_radius(4.0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_thumbnail_cache_reuses_only_the_same_picture_and_drops_missing_images() {
        let context = egui::Context::default();
        let downloader = Arc::new(crate::youtube::scripted::Scripted::new(
            PathBuf::new(),
            true,
        ));
        let mut flow = Flow::new(Jobs::new(downloader, None, Arc::new(|| {})));
        let first = crate::youtube::scripted::preview().thumbnail;
        let second = crate::youtube::scripted::preview().thumbnail;
        let mut paint = |state: &ThumbnailState| {
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                flow.confirm_thumbnail(ui, state, "A thumbnail");
            });
            let freed = output
                .textures_delta
                .free
                .iter()
                .copied()
                .collect::<Vec<_>>();
            output.textures_delta.clear();
            (
                flow.thumbnail.as_ref().map(|cache| cache.texture.id()),
                freed,
            )
        };
        let original = paint(&first).0.unwrap();
        assert_eq!(paint(&first.clone()), (Some(original), Vec::new()));
        let (replacement, freed) = paint(&second);
        let replacement = replacement.unwrap();
        assert_eq!(freed, [original]);
        assert_ne!(replacement, original);
        assert_eq!(paint(&ThumbnailState::Missing), (None, vec![replacement]));
        assert_eq!(
            paint(&ThumbnailState::Unavailable(
                deadpan_cli::youtube::thumbnail::Unavailable::Network
            )),
            (None, Vec::new())
        );
        assert_ne!(paint(&first).0.unwrap(), original);
    }
}
