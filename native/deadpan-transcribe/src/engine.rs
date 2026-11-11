//! whisper.cpp recognition shared by the worker and the word-timing example.

use deadpan_analysis::{RawSegment, RawToken};
use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
};

/// One token with both timestamp families, in centiseconds from the PCM start.
#[derive(Debug, Clone)]
#[allow(dead_code)] // the example prints every field
pub struct Token {
    pub text: String,
    pub t0: i64,
    pub t1: i64,
    pub t_dtw: i64,
    pub p: f32,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub t0: i64,
    pub t1: i64,
    pub tokens: Vec<Token>,
}

pub fn load(path: &str, dtw: bool) -> Result<WhisperContext, String> {
    let mut parameters = WhisperContextParameters::default();
    // The only approved pack is base.en, whose alignment heads are built in.
    // DTW requires flash attention to stay off (the default).
    if dtw {
        parameters.dtw_parameters = DtwParameters {
            mode: DtwMode::ModelPreset {
                model_preset: DtwModelPreset::BaseEn,
            },
            ..DtwParameters::default()
        };
    }
    WhisperContext::new_with_params(path, parameters)
        .map_err(|error| format!("load model: {error}"))
}

pub fn recognize(
    context: &WhisperContext,
    pcm: &[f32],
    language: &str,
    configure: impl FnOnce(&mut FullParams<'_, '_>),
) -> Result<Vec<Segment>, String> {
    let mut state = context
        .create_state()
        .map_err(|error| format!("create recognizer state: {error}"))?;
    let mut parameters = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    parameters.set_token_timestamps(true);
    parameters.set_language(Some(language));
    parameters.set_print_progress(false);
    parameters.set_print_realtime(false);
    parameters.set_print_special(false);
    parameters.set_print_timestamps(false);
    configure(&mut parameters);
    state
        .full(parameters, pcm)
        .map_err(|error| format!("recognize: {error}"))?;
    let mut segments = Vec::new();
    for segment in state.as_iter() {
        let mut tokens = Vec::new();
        for index in 0..segment.n_tokens() {
            let Some(token) = segment.get_token(index) else {
                continue;
            };
            let data = token.token_data();
            tokens.push(Token {
                text: token
                    .to_str_lossy()
                    .map_err(|error| format!("token text: {error}"))?
                    .into_owned(),
                t0: data.t0,
                t1: data.t1,
                t_dtw: data.t_dtw,
                p: data.p,
            });
        }
        segments.push(Segment {
            t0: segment.start_timestamp(),
            t1: segment.end_timestamp(),
            tokens,
        });
    }
    Ok(segments)
}

/// Fraction of the gap to the previous token's DTW time at which a token
/// starts, and of the gap to the next token's at which it ends. whisper.cpp's
/// DTW time lies late inside its token (median +140 ms from the true start on
/// LibriSpeech word alignments); these fractions were fitted on one sample of
/// 200 utterances and hold on another (median start error 48 ms, end 46 ms).
const START_BACK_TENTHS: i64 = 6;
const END_FORWARD_TENTHS: i64 = 4;

/// Token bounds for one segment from DTW times, or `None` when whisper.cpp
/// produced none (a segment without timed text falls back to its heuristic
/// bounds). Special tokens keep their original bounds; they are dropped later.
fn dtw_bounds(segment: &Segment) -> Option<Vec<(i64, i64)>> {
    let timed: Vec<usize> = (0..segment.tokens.len())
        .filter(|&i| {
            let text = &segment.tokens[i].text;
            !text.starts_with("[_") && !text.trim().is_empty()
        })
        .collect();
    if timed.is_empty() || timed.iter().any(|&i| segment.tokens[i].t_dtw < 0) {
        return None;
    }
    let mut bounds: Vec<(i64, i64)> = segment.tokens.iter().map(|t| (t.t0, t.t1)).collect();
    let mut previous = segment.t0;
    for (n, &i) in timed.iter().enumerate() {
        let time = segment.tokens[i].t_dtw.max(previous);
        let next = timed
            .get(n + 1)
            .map_or(segment.t1, |&j| segment.tokens[j].t_dtw)
            .max(time);
        let start = time - ((time - previous) * START_BACK_TENTHS + 5) / 10;
        let end = time + ((next - time) * END_FORWARD_TENTHS + 5) / 10;
        bounds[i] = (start, end);
        previous = time;
    }
    Some(bounds)
}

/// Worker output: DTW-derived bounds where whisper.cpp supplied them.
pub fn to_raw(segments: &[Segment]) -> Vec<RawSegment> {
    segments
        .iter()
        .map(|segment| {
            let bounds = dtw_bounds(segment);
            RawSegment {
                t0: segment.t0,
                t1: segment.t1,
                tokens: segment
                    .tokens
                    .iter()
                    .enumerate()
                    .map(|(i, token)| {
                        let (t0, t1) = bounds.as_ref().map_or((token.t0, token.t1), |b| b[i]);
                        RawToken {
                            text: token.text.clone(),
                            t0,
                            t1,
                            p: token.p,
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str, t0: i64, t1: i64, t_dtw: i64) -> Token {
        Token {
            text: text.into(),
            t0,
            t1,
            t_dtw,
            p: 0.9,
        }
    }

    #[test]
    fn dtw_times_become_start_and_end_fractions() {
        let segment = Segment {
            t0: 0,
            t1: 300,
            tokens: vec![
                token("[_BEG_]", 0, 0, -1),
                token(" The", 0, 24, 60),
                token(" sentence", 58, 88, 100),
            ],
        };
        let raw = to_raw(&[segment]);
        // Previous time is the segment start, then 60.
        assert_eq!((raw[0].tokens[1].t0, raw[0].tokens[1].t1), (24, 76));
        assert_eq!((raw[0].tokens[2].t0, raw[0].tokens[2].t1), (76, 180));
        assert_eq!((raw[0].tokens[0].t0, raw[0].tokens[0].t1), (0, 0));
    }

    #[test]
    fn a_segment_without_dtw_times_keeps_heuristic_bounds() {
        let segment = Segment {
            t0: 0,
            t1: 90,
            tokens: vec![token(" Hi", 5, 30, -1)],
        };
        let raw = to_raw(&[segment]);
        assert_eq!((raw[0].tokens[0].t0, raw[0].tokens[0].t1), (5, 30));
    }
}
