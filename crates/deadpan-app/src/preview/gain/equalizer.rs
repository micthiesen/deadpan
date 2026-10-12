//! Buffered EQ fields share the existing captured audio draft and audition.
use super::{FocusReveal, field};
use crate::gain::{GainEdit, format_db, parse_eq_cut, parse_eq_db};
use deadpan_core::{AudioTreatmentStage, EqualizerRecipe};
use eframe::egui;

pub(super) struct Fields {
    bands: [String; 3],
    low: String,
    high: String,
    error: Option<String>,
}
impl Fields {
    pub(super) fn new(edit: &GainEdit) -> Self {
        let recipe = edit.recipe().equalizer().unwrap_or_default();
        Self {
            bands: recipe.gains().map(format_db),
            low: recipe
                .low_cut()
                .map_or_else(|| "off".into(), |cut| cut.hz().to_string()),
            high: recipe
                .high_cut()
                .map_or_else(|| "off".into(), |cut| cut.hz().to_string()),
            error: None,
        }
    }
    fn parsed(&self) -> Result<EqualizerRecipe, String> {
        EqualizerRecipe::new(
            [
                parse_eq_db(&self.bands[0])?,
                parse_eq_db(&self.bands[1])?,
                parse_eq_db(&self.bands[2])?,
            ],
            parse_eq_cut(&self.low)?,
            parse_eq_cut(&self.high)?,
        )
        .map_err(|e| e.to_string())
    }
    pub(super) fn ready(&self, edit: &GainEdit) -> bool {
        self.error.is_none()
            && self
                .parsed()
                .is_ok_and(|recipe| recipe == edit.recipe().equalizer().unwrap_or_default())
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui, edit: &mut GainEdit) -> bool {
        let before = edit.recipe().clone();
        ui.weak("Bass / middle / treble · −24 to +24 dB · cuts 80–20000 Hz or off");
        ui.horizontal_wrapped(|ui| {
            for (index, (label, name)) in [
                ("Bass · dB", "eq-bass"),
                ("Middle · dB", "eq-middle"),
                ("Treble · dB", "eq-treble"),
            ]
            .into_iter()
            .enumerate()
            {
                field(ui, label, name, &mut self.bands[index], 72.0);
            }
        });
        ui.horizontal_wrapped(|ui| {
            field(ui, "Low cut · Hz / off", "eq-low", &mut self.low, 80.0);
            field(ui, "High cut · Hz / off", "eq-high", &mut self.high, 80.0);
            if ui.button("Set EQ").reveal_on_focus().clicked() {
                match self
                    .parsed()
                    .and_then(|recipe| edit.set_equalizer(Some(recipe)))
                {
                    Ok(()) => *self = Self::new(edit),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    edit.recipe().equalizer().is_some(),
                    egui::Button::new("Remove EQ"),
                )
                .reveal_on_focus()
                .clicked()
            {
                match edit.set_equalizer(None) {
                    Ok(()) => *self = Self::new(edit),
                    Err(error) => self.error = Some(error),
                }
            }
        });
        let order = edit
            .recipe()
            .order()
            .iter()
            .map(|stage| match stage {
                AudioTreatmentStage::ClipGain => "Gain",
                AudioTreatmentStage::Equalizer => "EQ",
                AudioTreatmentStage::Saturation => "Saturation",
                AudioTreatmentStage::StereoBalance => "Balance",
            })
            .collect::<Vec<_>>()
            .join(" → ");
        ui.weak(if order.is_empty() {
            "No authored treatment stages".into()
        } else {
            format!("Order: {order}")
        });
        if edit.recipe().equalizer().is_none() {
            ui.small("No EQ stage. Set EQ adds these settings to the draft.");
        }
        ui.horizontal_wrapped(|ui| {
            ui.small("Tab: fields and buttons · Enter: activate focused button");
            if !self.ready(edit) {
                ui.colored_label(egui::Color32::from_rgb(220, 187, 110), "Unapplied fields");
                if ui.button("Reset fields").reveal_on_focus().clicked() {
                    *self = Self::new(edit);
                }
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        edit.recipe() != &before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eq_fields_preserve_absence_validate_together_and_keep_other_treatments() {
        let mut edit = GainEdit::new(
            deadpan_core::AudioTreatments::default()
                .with_saturation(Some(
                    deadpan_core::Saturation::new(deadpan_core::GainDb::new(6000).unwrap())
                        .unwrap(),
                ))
                .unwrap(),
        );
        let before = edit.recipe().clone();
        let mut fields = Fields::new(&edit);
        assert!(fields.ready(&edit));
        assert!(edit.recipe().equalizer().is_none());
        fields.bands[1] = "6.125".into();
        fields.low = "80".into();
        fields.high = "12000".into();
        assert!(!fields.ready(&edit));
        edit.set_equalizer(Some(fields.parsed().unwrap())).unwrap();
        assert!(fields.ready(&edit));
        assert_eq!(edit.recipe().saturation(), before.saturation());
        assert_eq!(
            edit.recipe().order(),
            [
                AudioTreatmentStage::Equalizer,
                AudioTreatmentStage::Saturation
            ]
        );
        let valid = edit.recipe().clone();
        fields.low = "15000".into();
        assert!(fields.parsed().is_err());
        assert!(!fields.ready(&edit));
        assert_eq!(edit.recipe(), &valid);
        edit.set_equalizer(None).unwrap();
        assert_eq!(edit.recipe(), &before);
    }
}
