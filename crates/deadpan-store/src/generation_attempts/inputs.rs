//! Operation-specific immutable conditioning objects retained with a candidate.

use deadpan_core::GeneratedObjectRef;
use deadpan_jobs::Sha256;
use serde::{Deserialize, Deserializer, Serialize};

use super::AttemptValueError;

pub const MAX_EXTENSION_INPUTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "InputWire")]
pub struct BundleInputObjects {
    #[serde(flatten)]
    value: InputWire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum InputWire {
    Bridge {
        context_sha256: Sha256,
        manifest: GeneratedObjectRef,
        left: GeneratedObjectRef,
        right: GeneratedObjectRef,
    },
    Extension {
        context_sha256: Sha256,
        manifest: GeneratedObjectRef,
        #[serde(deserialize_with = "bounded_context")]
        context: Vec<GeneratedObjectRef>,
        #[serde(deserialize_with = "required_option")]
        opposite: Option<GeneratedObjectRef>,
        signatures: GeneratedObjectRef,
    },
}

impl TryFrom<InputWire> for BundleInputObjects {
    type Error = AttemptValueError;

    fn try_from(value: InputWire) -> Result<Self, Self::Error> {
        let inputs = Self { value };
        inputs.validate()?;
        Ok(inputs)
    }
}

impl BundleInputObjects {
    pub fn new(
        context_sha256: Sha256,
        manifest: GeneratedObjectRef,
        left: GeneratedObjectRef,
        right: GeneratedObjectRef,
    ) -> Result<Self, AttemptValueError> {
        InputWire::Bridge {
            context_sha256,
            manifest,
            left,
            right,
        }
        .try_into()
    }

    pub fn new_extension(
        context_sha256: Sha256,
        manifest: GeneratedObjectRef,
        context: Vec<GeneratedObjectRef>,
        opposite: Option<GeneratedObjectRef>,
        signatures: GeneratedObjectRef,
    ) -> Result<Self, AttemptValueError> {
        InputWire::Extension {
            context_sha256,
            manifest,
            context,
            opposite,
            signatures,
        }
        .try_into()
    }

    pub fn context_sha256(&self) -> &Sha256 {
        match &self.value {
            InputWire::Bridge { context_sha256, .. }
            | InputWire::Extension { context_sha256, .. } => context_sha256,
        }
    }

    pub fn manifest(&self) -> &GeneratedObjectRef {
        match &self.value {
            InputWire::Bridge { manifest, .. } | InputWire::Extension { manifest, .. } => manifest,
        }
    }

    pub fn left(&self) -> Option<&GeneratedObjectRef> {
        match &self.value {
            InputWire::Bridge { left, .. } => Some(left),
            _ => None,
        }
    }

    pub fn right(&self) -> Option<&GeneratedObjectRef> {
        match &self.value {
            InputWire::Bridge { right, .. } => Some(right),
            _ => None,
        }
    }

    /// Chronological extension inputs. Bridge inputs have no temporal context array.
    pub fn context(&self) -> Option<&[GeneratedObjectRef]> {
        match &self.value {
            InputWire::Extension { context, .. } => Some(context),
            _ => None,
        }
    }

    pub fn opposite(&self) -> Option<&GeneratedObjectRef> {
        match &self.value {
            InputWire::Extension { opposite, .. } => opposite.as_ref(),
            _ => None,
        }
    }

    pub fn signatures(&self) -> Option<&GeneratedObjectRef> {
        match &self.value {
            InputWire::Extension { signatures, .. } => Some(signatures),
            _ => None,
        }
    }

    /// Complete dependencies, preserving chronological context and repeated pictures.
    pub fn objects(&self) -> impl Iterator<Item = &GeneratedObjectRef> {
        [Some(self.manifest()), self.left(), self.right()]
            .into_iter()
            .flatten()
            .chain(self.context().unwrap_or(&[]))
            .chain(self.opposite())
            .chain(self.signatures())
    }

    fn validate(&self) -> Result<(), AttemptValueError> {
        if self
            .context()
            .is_some_and(|context| context.is_empty() || context.len() > MAX_EXTENSION_INPUTS)
        {
            return Err(AttemptValueError::BundleMetadataMismatch);
        }
        let objects: Vec<_> = self.objects().collect();
        for (index, object) in objects.iter().enumerate() {
            for previous in &objects[..index] {
                if object.content() == previous.content()
                    && (object != previous
                        || previous.content() == self.manifest().content()
                        || self
                            .signatures()
                            .is_some_and(|signatures| object.content() == signatures.content()))
                {
                    return Err(AttemptValueError::BundleMetadataMismatch);
                }
            }
        }
        Ok(())
    }
}

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

fn bounded_context<'de, D>(deserializer: D) -> Result<Vec<GeneratedObjectRef>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Context;
    impl<'de> serde::de::Visitor<'de> for Context {
        type Value = Vec<GeneratedObjectRef>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 64 chronological extension inputs")
        }
        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut inputs = Vec::new();
            while inputs.len() < MAX_EXTENSION_INPUTS {
                let Some(input) = sequence.next_element()? else {
                    return Ok(inputs);
                };
                inputs.push(input);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom(
                    "extension inputs exceed 64 pictures",
                ));
            }
            Ok(inputs)
        }
    }
    deserializer.deserialize_seq(Context)
}
