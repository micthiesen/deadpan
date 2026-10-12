//! Bounded static equalizer parameters. PCM and coefficient construction belong
//! to the audio layer. AudioTreatments stores its explicit position among the
//! admitted gain, saturation and balance stages.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::GainDb;

pub const EQ_MIN_CUTOFF_HZ: u16 = 80;
pub const EQ_MAX_CUTOFF_HZ: u16 = 20_000;
pub const EQ_MIN_PASSBAND_HZ: u16 = 80;
pub const EQ_MAX_GAIN_MILLIDECIBELS: i32 = 24_000;
pub const EQ_BASS_CROSSOVER_HZ: u16 = 250;
pub const EQ_TREBLE_CROSSOVER_HZ: u16 = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualizerRecipeError {
    Cutoff,
    Gain,
    Passband,
}

impl fmt::Display for EqualizerRecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cutoff => "EQ cutoffs must be integer Hz from 80 through 20000",
            Self::Gain => "EQ band gains must be between -24 dB and +24 dB",
            Self::Passband => "EQ high cut must be at least 80 Hz above its low cut",
        })
    }
}
impl Error for EqualizerRecipeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct EqCutoffHz(u16);

impl EqCutoffHz {
    pub fn new(hz: u16) -> Result<Self, EqualizerRecipeError> {
        if !(EQ_MIN_CUTOFF_HZ..=EQ_MAX_CUTOFF_HZ).contains(&hz) {
            return Err(EqualizerRecipeError::Cutoff);
        }
        Ok(Self(hz))
    }
    pub const fn hz(self) -> u16 {
        self.0
    }
}
impl TryFrom<u16> for EqCutoffHz {
    type Error = EqualizerRecipeError;
    fn try_from(hz: u16) -> Result<Self, Self::Error> {
        Self::new(hz)
    }
}
impl From<EqCutoffHz> for u16 {
    fn from(hz: EqCutoffHz) -> Self {
        hz.0
    }
}

/// Three complementary linear-phase bands (bass below 250 Hz, middle from
/// 250 Hz through 4 kHz, treble above 4 kHz) and optional low/high cuts. Values
/// are exact authored parameters, not normalized loudness targets. Each gain
/// admits ±24 dB; no value means mute. Cutoffs name the finite filter transition
/// centers, not brick-wall boundaries. The audio layer versions its response.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Wire", into = "Wire")]
pub struct EqualizerRecipe {
    gains: [GainDb; 3],
    low_cut: Option<EqCutoffHz>,
    high_cut: Option<EqCutoffHz>,
}

impl EqualizerRecipe {
    pub fn new(
        gains: [GainDb; 3],
        low_cut: Option<EqCutoffHz>,
        high_cut: Option<EqCutoffHz>,
    ) -> Result<Self, EqualizerRecipeError> {
        if gains
            .iter()
            .any(|gain| gain.millidecibels().abs() > EQ_MAX_GAIN_MILLIDECIBELS)
        {
            return Err(EqualizerRecipeError::Gain);
        }
        if let (Some(low), Some(high)) = (low_cut, high_cut)
            && high
                .hz()
                .checked_sub(low.hz())
                .is_none_or(|width| width < EQ_MIN_PASSBAND_HZ)
        {
            return Err(EqualizerRecipeError::Passband);
        }
        Ok(Self {
            gains,
            low_cut,
            high_cut,
        })
    }
    pub const fn gains(self) -> [GainDb; 3] {
        self.gains
    }
    /// A low cut removes low frequencies (a high-pass response).
    pub const fn low_cut(self) -> Option<EqCutoffHz> {
        self.low_cut
    }
    /// A high cut removes high frequencies (a low-pass response).
    pub const fn high_cut(self) -> Option<EqCutoffHz> {
        self.high_cut
    }
    pub fn is_unity(self) -> bool {
        self == Self::default()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    bass: GainDb,
    middle: GainDb,
    treble: GainDb,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    low_cut_hz: Option<EqCutoffHz>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    high_cut_hz: Option<EqCutoffHz>,
}
impl TryFrom<Wire> for EqualizerRecipe {
    type Error = EqualizerRecipeError;
    fn try_from(value: Wire) -> Result<Self, Self::Error> {
        Self::new(
            [value.bass, value.middle, value.treble],
            value.low_cut_hz,
            value.high_cut_hz,
        )
    }
}
impl From<EqualizerRecipe> for Wire {
    fn from(value: EqualizerRecipe) -> Self {
        Self {
            bass: value.gains[0],
            middle: value.gains[1],
            treble: value.gains[2],
            low_cut_hz: value.low_cut,
            high_cut_hz: value.high_cut,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_recipe_round_trips_and_neutral_is_a_distinct_bypass() {
        let recipe = EqualizerRecipe::new(
            [
                GainDb::new(-24000).unwrap(),
                GainDb::new(7500).unwrap(),
                GainDb::new(24000).unwrap(),
            ],
            Some(EqCutoffHz::new(80).unwrap()),
            Some(EqCutoffHz::new(20000).unwrap()),
        )
        .unwrap();
        let json = serde_json::to_string(&recipe).unwrap();
        assert_eq!(
            serde_json::from_str::<EqualizerRecipe>(&json).unwrap(),
            recipe
        );
        assert_eq!(recipe.gains()[1].millidecibels(), 7500);
        assert!(!recipe.is_unity());
        assert!(EqualizerRecipe::default().is_unity());
        let cut =
            EqualizerRecipe::new([GainDb::UNITY; 3], Some(EqCutoffHz::new(80).unwrap()), None)
                .unwrap();
        assert!(!cut.is_unity());
    }
    #[test]
    fn typed_and_wire_ingress_reject_invalid_frequency_gain_and_reversed_cuts() {
        for hz in [0, 79, 20001, u16::MAX] {
            assert_eq!(EqCutoffHz::new(hz), Err(EqualizerRecipeError::Cutoff));
        }
        let unity = [GainDb::UNITY; 3];
        assert_eq!(
            EqualizerRecipe::new([GainDb::new(-24001).unwrap(); 3], None, None),
            Err(EqualizerRecipeError::Gain)
        );
        for (low, high) in [(1000, 1000), (2000, 1000), (1000, 1079)] {
            assert_eq!(
                EqualizerRecipe::new(
                    unity,
                    Some(EqCutoffHz::new(low).unwrap()),
                    Some(EqCutoffHz::new(high).unwrap())
                ),
                Err(EqualizerRecipeError::Passband)
            );
        }
        assert!(
            EqualizerRecipe::new(
                unity,
                Some(EqCutoffHz::new(1000).unwrap()),
                Some(EqCutoffHz::new(1080).unwrap())
            )
            .is_ok()
        );
        for json in [
            r#"{"bass":0,"middle":0,"treble":0,"low_cut_hz":-1}"#,
            r#"{"bass":0,"middle":0,"treble":0,"high_cut_hz":24000}"#,
            r#"{"bass":-96000,"middle":0,"treble":0}"#,
            r#"{"bass":0,"middle":0,"treble":0,"low_cut_hz":1000,"high_cut_hz":1050}"#,
            r#"{"bass":0,"middle":0,"treble":0,"automatic_normalization":true}"#,
        ] {
            assert!(
                serde_json::from_str::<EqualizerRecipe>(json).is_err(),
                "{json}"
            );
        }
    }
}
