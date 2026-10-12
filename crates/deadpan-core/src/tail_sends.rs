//! Explicit sends into allocated hanging-tail pauses. Sends follow an owner's
//! treatments and balance; they do not change its dry signal or allocate time.

use serde::{Deserialize, Serialize};

use crate::{GainDb, TailEffect};

/// Two closed, optional sends. `None` is off; a zero dB send is unity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TailSends {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reverb: Option<GainDb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delay: Option<GainDb>,
}

impl TailSends {
    pub fn level(self, effect: TailEffect) -> Option<GainDb> {
        match effect {
            TailEffect::Reverb => self.reverb,
            TailEffect::Delay => self.delay,
        }
    }

    pub fn with_level(mut self, effect: TailEffect, level: Option<GainDb>) -> Self {
        match effect {
            TailEffect::Reverb => self.reverb = level,
            TailEffect::Delay => self.delay = level,
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.reverb.is_none() && self.delay.is_none()
    }

    pub fn record_count(&self) -> usize {
        usize::from(self.reverb.is_some()) + usize::from(self.delay.is_some())
    }
}

/// Which processed input feeds a hanging tail's fixed preceding window.
/// Both choices suppress other tail returns and keep dry audio out of the pause.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum TailInput {
    #[default]
    Original,
    Sends,
}

impl TailInput {
    pub fn is_original(&self) -> bool {
        *self == Self::Original
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioTreatments, ClipGain, Saturation, StereoBalance};

    #[test]
    fn wire_is_closed_and_levels_are_exact_and_bounded() {
        let sends = TailSends::default()
            .with_level(TailEffect::Reverb, Some(GainDb::new(-6001).unwrap()))
            .with_level(TailEffect::Delay, Some(GainDb::UNITY));
        assert_eq!(
            serde_json::to_string(&sends).unwrap(),
            r#"{"reverb":-6001,"delay":0}"#
        );
        assert_eq!(
            serde_json::from_str::<TailSends>(&serde_json::to_string(&sends).unwrap()).unwrap(),
            sends
        );
        for bad in [
            r#"{"reverb":24001}"#,
            r#"{"delay":-96001}"#,
            r#"{"delay":0.1}"#,
            r#"{"delay":0,"delay":1}"#,
            r#"{"chorus":0}"#,
            r#"{"reverb":[]}"#,
        ] {
            assert!(serde_json::from_str::<TailSends>(bad).is_err(), "{bad}");
        }
        assert_eq!(sends.record_count(), 2);
        assert!(
            sends
                .with_level(TailEffect::Reverb, None)
                .with_level(TailEffect::Delay, None)
                .is_empty()
        );
    }

    #[test]
    fn changing_other_treatments_keeps_sends_and_removing_a_send_keeps_stages() {
        let sends = TailSends::default().with_level(TailEffect::Reverb, Some(GainDb::UNITY));
        let recipe = AudioTreatments::default().with_tail_sends(sends);
        assert!(!recipe.is_empty());
        assert_eq!(recipe.record_count(), 1);
        let gain = ClipGain::new(GainDb::new(-6000).unwrap(), false, vec![], vec![]).unwrap();
        let recipe = recipe
            .with_clip_gain(gain)
            .unwrap()
            .with_saturation(Some(Saturation::new(GainDb::UNITY).unwrap()))
            .unwrap()
            .with_balance(Some(StereoBalance::new(25).unwrap()))
            .unwrap();
        assert_eq!(recipe.tail_sends(), sends);
        let removed = recipe.clone().with_tail_sends(TailSends::default());
        assert_eq!(removed.order(), recipe.order());
        assert_eq!(removed.clip_gain(), recipe.clip_gain());
        let cleared = recipe
            .with_clip_gain_or_none(ClipGain::default())
            .unwrap()
            .with_saturation(None)
            .unwrap()
            .with_balance(None)
            .unwrap();
        assert_eq!(cleared.tail_sends(), sends);
        assert!(cleared.order().is_empty());
        assert_eq!(
            AudioTreatments::from_json(&cleared.to_json().unwrap()).unwrap(),
            cleared
        );
    }
}
