#![no_main]
//! Gag recipes (selector 0: recipe JSON) and pinning group labels from
//! project files (selector 1). Expandable recipes' labels parse back.
use deadpan_core::{FrameRate, GagRecipe};
use deadpan_fuzz::{refused, select};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let (selector, body) = select(input);
    let recipe = if selector % 2 == 0 {
        match serde_json::from_slice::<GagRecipe>(body) {
            Ok(recipe) => recipe,
            Err(error) => return refused(error),
        }
    } else {
        let Ok(label) = std::str::from_utf8(body) else { return };
        match GagRecipe::from_label(label) {
            Some(recipe) => recipe,
            None => return,
        }
    };
    let mut expanded = false;
    for (numerator, denominator) in [(30, 1), (24_000, 1_001), (240, 1)] {
        let rate = FrameRate::new(numerator, denominator).unwrap();
        for visual in [false, true] {
            match recipe.expand(visual, rate) {
                Ok(_) => expanded = true,
                Err(error) => refused(error),
            }
        }
    }
    if expanded {
        assert!(
            GagRecipe::from_label(&recipe.label()).is_some(),
            "expandable recipe label does not parse: {}",
            recipe.label()
        );
    }
});
