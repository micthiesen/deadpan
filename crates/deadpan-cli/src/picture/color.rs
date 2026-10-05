//! Automatic SDR/HDR branch: the pure rule is `deadpan_core::decide_output_color`;
//! this module maps receipts onto it and the decision onto the shared renderer.

pub use deadpan_core::{
    AssetColor, AssetTransfer, DEFAULT_HDR_PEAK_NITS, MIN_TRUSTED_CONTENT_LIGHT_NITS,
    OutputColorDecision, OutputColorReason, decide_output_color,
};
pub use deadpan_media::output_color::asset_color;
use deadpan_render::{ColorPipeline, HdrTransfer, OutputColor, ToneMap};

/// The renderer configuration shared by preview and export.
pub trait ColorDecisionPipeline {
    fn pipeline(&self) -> ColorPipeline;
}

impl ColorDecisionPipeline for OutputColorDecision {
    fn pipeline(&self) -> ColorPipeline {
        let tone_map =
            ToneMap::new(f64::from(self.tone_map_peak_nits)).unwrap_or_else(|_| ToneMap::default());
        let output = match self.output {
            deadpan_core::ColorPolicy::SdrRec709 => OutputColor::Sdr,
            deadpan_core::ColorPolicy::HdrRec2020Pq => OutputColor::Hdr(HdrTransfer::Pq),
            deadpan_core::ColorPolicy::HdrRec2020Hlg => OutputColor::Hdr(HdrTransfer::Hlg),
        };
        ColorPipeline { output, tone_map }
    }
}
