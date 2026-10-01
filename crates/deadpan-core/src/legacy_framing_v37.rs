//! Closed framing vocabulary before schema 38 introduced retained owner clocks.

use serde::{Deserialize, Serialize};

use crate::{Framing, FramingClock, FramingValue};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyFraming {
    value: FramingValue,
}

impl LegacyFraming {
    pub(crate) fn upgrade(self) -> Framing {
        Framing {
            clock: FramingClock::OwnerOutput,
            value: self.value,
        }
    }

    pub(crate) fn project(framing: &Framing) -> Option<Self> {
        matches!(framing.clock, FramingClock::OwnerOutput).then(|| Self {
            value: framing.value.clone(),
        })
    }
}
