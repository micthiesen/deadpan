//! Borrowed ordinary-provider witness only; never a constructor.
use super::*;
impl SourceVoiceProvider {
    pub(in crate::plan) fn provider_before_snapshot(&self) -> serde_json::Value {
        let Self { audio, recipe } = self;
        let CompiledSourceAudio {
            source,
            start,
            duration,
            selection,
            selected,
        } = audio;
        let AudioSourceVoiceRecipe {
            source: recipe_source,
            mapping,
            offset,
        } = recipe;
        serde_json::json!({"audio":{"source":source,"start":start,"duration":duration,
            "selection":selection,"selected":selected},
            "recipe":{"source":recipe_source,"mapping":mapping,"offset":offset}})
    }
}
