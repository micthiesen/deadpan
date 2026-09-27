//! Frozen recipe vocabulary shared by core schemas 5 through 18. Keep this
//! adapter on every historical recipe field, including command and patch nodes.

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::Error};

use crate::{FrameDuration, HoldAudio, HoldRecipe, HoldVideo};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    duration: FrameDuration,
    video: HoldVideo,
    audio: HoldAudio,
}

impl From<Recipe> for HoldRecipe {
    fn from(recipe: Recipe) -> Self {
        Self {
            duration: recipe.duration,
            video: recipe.video,
            audio: recipe.audio,
            picture_context: None,
        }
    }
}

#[derive(Serialize)]
struct RecipeRef<'a> {
    duration: FrameDuration,
    video: &'a HoldVideo,
    audio: &'a HoldAudio,
}

fn project<E: Error>(recipe: &HoldRecipe) -> Result<RecipeRef<'_>, E> {
    if recipe.picture_context.is_some() {
        return Err(E::custom(
            "historical Hold recipe cannot contain picture_context",
        ));
    }
    Ok(RecipeRef {
        duration: recipe.duration,
        video: &recipe.video,
        audio: &recipe.audio,
    })
}

pub(crate) mod recipe {
    use super::*;

    pub fn deserialize<'de, D: Deserializer<'de>>(decoder: D) -> Result<HoldRecipe, D::Error> {
        Recipe::deserialize(decoder).map(Into::into)
    }

    pub fn serialize<S: Serializer>(recipe: &HoldRecipe, encoder: S) -> Result<S::Ok, S::Error> {
        project::<S::Error>(recipe)?.serialize(encoder)
    }
}

pub(crate) mod optional {
    use super::*;

    pub fn deserialize<'de, D: Deserializer<'de>>(
        decoder: D,
    ) -> Result<Option<HoldRecipe>, D::Error> {
        Option::<Recipe>::deserialize(decoder).map(|value| value.map(Into::into))
    }

    pub fn serialize<S: Serializer>(
        recipe: &Option<HoldRecipe>,
        encoder: S,
    ) -> Result<S::Ok, S::Error> {
        recipe
            .as_ref()
            .map(project::<S::Error>)
            .transpose()?
            .serialize(encoder)
    }
}
