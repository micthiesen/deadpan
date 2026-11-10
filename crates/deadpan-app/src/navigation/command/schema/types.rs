//! Typed argument values. Each check calls the value parser that the verb's
//! execution path uses, never a second grammar.

use super::Type;
use crate::navigation::{duration::DurationInput, escalation, gag, retime, slip, trim, zoom};

fn positive_count(value: &str, minimum: u32, maximum: u32) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value
            .parse::<u32>()
            .is_ok_and(|count| (minimum..=maximum).contains(&count))
}

fn signed(value: &str) -> &str {
    value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value)
}

pub static DURATION: Type = Type {
    name: "duration",
    units: "12f, 250ms, 1.5s or 01:02.500",
    check: |value| DurationInput::parse(value).is_ok(),
};

pub static NONZERO_DURATION: Type = Type {
    name: "duration",
    units: "12f, 250ms, 1.5s or 01:02.500, above zero",
    check: |value| crate::navigation::hold_effects::parse_split(&[value]).is_ok(),
};

pub static GAP: Type = Type {
    name: "duration",
    units: "6f, 120ms or 0 for no gaps",
    check: |value| value == "0" || DurationInput::parse(value).is_ok(),
};

pub static SIGNED_DURATION: Type = Type {
    name: "signed duration",
    units: "+80ms, -2f or 0",
    check: |value| super::super::audio_lag(Some(value)).is_ok(),
};

pub static DURATION_STEP: Type = Type {
    name: "signed duration",
    units: "-40ms or +2f",
    check: |value| DurationInput::parse(signed(value)).is_ok(),
};

pub static FRAMES: Type = Type {
    name: "whole frames",
    units: "11f, above zero",
    check: |value| {
        value.strip_suffix('f').is_some_and(|digits| {
            !digits.is_empty()
                && digits.bytes().all(|byte| byte.is_ascii_digit())
                && digits.parse::<i64>().is_ok_and(|frames| frames > 0)
        })
    },
};

pub static SIGNED_FRAMES: Type = Type {
    name: "signed whole frames",
    units: "+5f or -3f",
    check: |value| slip::parse_frames(value).is_ok(),
};

pub static TRIM_FRAMES: Type = Type {
    name: "signed whole frames",
    units: "+5f, -3f or 0f",
    check: |value| trim::parse_frames(value).is_ok(),
};

pub static SPEED: Type = Type {
    name: "speed",
    units: "0.75 or 3/4, above zero",
    check: |value| retime::speed(value).is_some(),
};

pub static SCALE: Type = Type {
    name: "scale",
    units: "1.35 or 2x, from 1/64 to 64",
    check: |value| zoom::scale(value).is_ok(),
};

pub static DECIMAL: Type = Type {
    name: "decimal",
    units: "1.35",
    check: |value| gag::decimal(value).is_ok(),
};

pub static ZOOM_STEP: Type = Type {
    name: "signed decimal",
    units: "0.08 or -0.05; 0 removes it",
    check: |value| escalation::decimal(value).is_ok(),
};

pub static GAIN_STEP: Type = Type {
    name: "level step",
    units: "3dB or -1.5dB; 0dB removes it",
    check: |value| escalation::gain(value).is_ok(),
};

pub static GAIN_LEVEL: Type = Type {
    name: "level",
    units: "+3dB or -6dB sets the trim, +=3dB or -=3dB changes it",
    check: |value| {
        let amount = value
            .strip_prefix("+=")
            .or_else(|| value.strip_prefix("-="))
            .unwrap_or(value);
        crate::gain::parse_db(super::super::strip_db(amount)).is_ok()
    },
};

pub static DRIVE: Type = Type {
    name: "drive",
    units: "12dB, from 0 to 24",
    check: |value| {
        crate::gain::parse_db(super::super::strip_db(value))
            .ok()
            .is_some_and(|drive| deadpan_core::Saturation::new(drive).is_ok())
    },
};

pub static SEMITONES: Type = Type {
    name: "pitch shift",
    units: "+5st or -2st, whole semitones within ±24",
    check: |value| retime::parse_semitones(value).is_ok(),
};

pub static SPEED_STEP: Type = Type {
    name: "percent",
    units: "10% faster, -10% slower, or 0.25; 0 removes it",
    check: |value| escalation::decimal(value.strip_suffix('%').unwrap_or(value)).is_ok(),
};

pub static COUNT: Type = Type {
    name: "whole count",
    units: "1 or more",
    check: |value| positive_count(value, 1, u32::MAX),
};

pub static TWO_OR_MORE: Type = Type {
    name: "whole count",
    units: "2 or more",
    check: |value| positive_count(value, 2, u32::MAX),
};

pub static RAMP_STEPS: Type = Type {
    name: "whole count",
    units: "2 to 32",
    check: |value| positive_count(value, 2, deadpan_core::MAX_RAMP_STEPS),
};

pub static VARIANT: Type = Type {
    name: "variant number",
    units: "1 to 255, as the inspector shows",
    check: |value| value.parse::<u8>().is_ok_and(|number| number > 0),
};

pub static VARY: Type = Type {
    name: "percentage",
    units: "1% to 50%",
    check: |value| gag::percent(value).is_ok(),
};

pub static SEED: Type = Type {
    name: "whole number",
    units: "7",
    check: |value| value.parse::<u64>().is_ok(),
};

pub static MONITOR: Type = Type {
    name: "percentage",
    units: "25%, from 0 to 100 with at most one decimal",
    check: |value| super::super::monitor(Some(value)).is_ok(),
};

pub static BALANCE: Type = Type {
    name: "signed percentage",
    units: "-50% (left) or +50% (right), within ±100%",
    check: |value| {
        value
            .strip_suffix('%')
            .and_then(|percent| percent.parse::<i16>().ok())
            .is_some_and(|percent| deadpan_core::StereoBalance::new(percent).is_ok())
    },
};

pub static FRAME_RANGE: Type = Type {
    name: "local frame range",
    units: "4-10, start before end",
    check: |value| {
        let frames =
            |value: &str| crate::gain::parse_frames(value.strip_suffix('f').unwrap_or(value));
        value
            .split_once('-')
            .and_then(|(start, end)| Some((frames(start).ok()?, frames(end).ok()?)))
            .is_some_and(|(start, end)| deadpan_core::GainRange::new(start, end).is_ok())
    },
};

pub static PLAY_LIST: Type = Type {
    name: "play list",
    units: "2-3 or 1,3, at least two plays",
    check: |value| super::super::scope_plays(value).is_ok(),
};

pub static GROUP_NAME: Type = Type {
    name: "JSON-quoted name",
    units: "\"the answer\", up to 1024 bytes",
    check: |value| {
        value.starts_with('"')
            && serde_json::from_str::<String>(value)
                .is_ok_and(|label| deadpan_core::validate_group_label(&label).is_ok())
    },
};

pub static TAG_LIST: Type = Type {
    name: "tag list",
    units: "reaction,breath: up to 16 lowercase words without spaces",
    check: |value| {
        deadpan_store::moments::MomentTags::from_input(
            value.split(',').filter(|tag| !tag.trim().is_empty()),
        )
        .is_ok()
    },
};

pub static VARIANTS: Type = Type {
    name: "variant count",
    units: "1 to 4",
    check: |value| {
        value
            .parse::<u8>()
            .is_ok_and(|count| (1..=crate::project::generation::MAX_VARIANTS).contains(&count))
    },
};

pub static TARGET_ID: Type = Type {
    name: "target id",
    units: "target-1",
    check: |value| deadpan_core::TargetId::new(value).is_ok(),
};

pub static NODE_ID: Type = Type {
    name: "node id",
    units: "an exact authored node id",
    check: |value| !value.is_empty() && deadpan_core::NodeId::new(value.to_owned()).is_ok(),
};

pub static MARK_ID: Type = Type {
    name: "mark id",
    units: "an exact mark id such as native-mark-a",
    check: |value| !value.is_empty() && deadpan_core::MarkId::new(value.to_owned()).is_ok(),
};

pub static NODE_LIST: Type = Type {
    name: "JSON node id list",
    units: "[\"id-a\",\"id-b\"], 1 to 64 distinct ids",
    check: |value| {
        super::super::parse(&format!("select nodes={value}"))
            .is_ok_and(|entry| matches!(entry, super::super::Entry::SelectNodes { .. }))
    },
};

pub static RANGE_LIST: Type = Type {
    name: "JSON range list",
    units: "[{\"start\":2,\"end\":6}], 1 to 64 relative frame ranges",
    check: |value| {
        super::super::parse(&format!("select ranges={value}"))
            .is_ok_and(|entry| matches!(entry, super::super::Entry::SelectRanges { .. }))
    },
};

pub static SEARCH_TEXT: Type = Type {
    name: "JSON-quoted words",
    units: "\"hello world\"",
    check: |value| {
        value.starts_with('"')
            && serde_json::from_str::<deadpan_core::SemanticTranscriptQuery>(value).is_ok()
    },
};

pub static EQ_BAND: Type = Type {
    name: "band level",
    units: "3dB or -6dB, within ±24 dB, with dB",
    check: |value| {
        value
            .strip_suffix("dB")
            .is_some_and(|level| crate::gain::parse_eq_db(level).is_ok())
    },
};

pub static EQ_CUT: Type = Type {
    name: "cutoff",
    units: "80Hz or 12000Hz, with Hz",
    check: |value| value.ends_with("Hz") && crate::gain::parse_eq_cut(value).is_ok(),
};

pub static ENVELOPE_FRAME: Type = Type {
    name: "local frame",
    units: "30f, from the sound's onset, with f",
    check: |value| {
        value
            .strip_suffix('f')
            .is_some_and(|frames| crate::gain::parse_frames(frames).is_ok())
    },
};

pub static ENVELOPE_LEVEL: Type = Type {
    name: "level",
    units: "-12dB or 0dB, with dB",
    check: |value| {
        value
            .strip_suffix("dB")
            .is_some_and(|level| crate::gain::parse_db(level).is_ok())
    },
};

pub static HERTZ: Type = Type {
    name: "frequency",
    units: "880Hz, from 20 Hz to 20 kHz",
    check: |value| {
        crate::navigation::hold_effects::hertz(value)
            .is_ok_and(|hertz| deadpan_core::TONE_FREQUENCY_HZ.contains(&hertz))
    },
};

pub static BLEEP_LEVEL: Type = Type {
    name: "level",
    units: "-6dB, at or below 0 dB",
    check: |value| {
        crate::navigation::hold_effects::parse_bleep(&[&format!("level={value}")]).is_ok()
    },
};

pub static SEND_LEVEL: Type = Type {
    name: "level",
    units: "-6dB, from -96 to +24 dB",
    check: |value| crate::gain::parse_db(value.strip_suffix("dB").unwrap_or(value)).is_ok(),
};

pub static SHIFT_FRAMES: Type = Type {
    name: "signed whole frames",
    units: "+5f or -3f, not zero",
    check: |value| {
        value
            .strip_suffix('f')
            .and_then(|digits| digits.parse::<i64>().ok())
            .is_some_and(|frames| frames != 0)
    },
};

pub static SAMPLE: Type = Type {
    name: "sample",
    units: "274000, whole 48 kHz samples from 0",
    check: |value| crate::navigation::sound::onset(Some(value)).is_ok(),
};

pub static SOUND_LEVEL: Type = Type {
    name: "level",
    units: "-3 or 1.5 dB, from -96 to 24, at most three decimals",
    check: |value| crate::navigation::sound::gain(Some(value)).is_ok(),
};
