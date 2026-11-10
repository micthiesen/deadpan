//! Keys of the waveform range sheet (`:wave`). The sheet is modal and has no
//! text fields: its letters never reach the editor, IME composition stays
//! native, and only movement keys act on a held (repeating) press.

use eframe::egui::{Key, Modifiers};

/// One mix sample is 1/48,000 s; a fine step is 1 ms.
pub const FINE_SAMPLES: i64 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveStep {
    /// Whole project frames, landing on the frame's exact mix boundary.
    Frames(i64),
    /// Exact mix samples.
    Samples(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveKey {
    /// Move the moving end (head) of the selection.
    Move(WaveStep),
    /// Move the head to the nearest zero crossing of the measured mix.
    ZeroCrossing,
    /// Exchange the fixed and moving ends.
    Swap,
    /// Start a new selection at the head.
    Restart,
    /// Play the selected samples once, or stop.
    Play,
    /// Mute exactly the selected samples of their beat.
    Mute,
    /// Change the selected samples' gain by 3 dB.
    Gain {
        louder: bool,
    },
    Close,
}

/// `background`: the sheet itself has focus, not one of its buttons, so
/// Space and Enter stay native button activation on a focused button.
pub fn route_key(
    key: Key,
    modifiers: Modifiers,
    background: bool,
    _unused: bool,
    ime: bool,
    repeat: bool,
) -> Option<WaveKey> {
    if ime {
        return None;
    }
    let plain = modifiers == Modifiers::NONE;
    let shift = modifiers == Modifiers::SHIFT;
    let movement = match key {
        Key::H | Key::ArrowLeft if plain => Some(WaveStep::Frames(-1)),
        Key::L | Key::ArrowRight if plain => Some(WaveStep::Frames(1)),
        Key::H | Key::ArrowLeft if shift => Some(WaveStep::Samples(-FINE_SAMPLES)),
        Key::L | Key::ArrowRight if shift => Some(WaveStep::Samples(FINE_SAMPLES)),
        Key::OpenBracket if plain => Some(WaveStep::Samples(-1)),
        Key::CloseBracket if plain => Some(WaveStep::Samples(1)),
        _ => None,
    };
    if let Some(step) = movement {
        return Some(WaveKey::Move(step));
    }
    if repeat {
        return None;
    }
    match key {
        Key::Escape if plain => Some(WaveKey::Close),
        Key::Z if plain => Some(WaveKey::ZeroCrossing),
        Key::O if plain => Some(WaveKey::Swap),
        Key::V if plain => Some(WaveKey::Restart),
        Key::M if plain => Some(WaveKey::Mute),
        Key::Space if plain && background => Some(WaveKey::Play),
        Key::Plus if plain || shift => Some(WaveKey::Gain { louder: true }),
        Key::Equals if shift => Some(WaveKey::Gain { louder: true }),
        Key::Minus if plain => Some(WaveKey::Gain { louder: false }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_keys_move_by_frame_millisecond_and_sample_and_leave_buttons_and_ime_native() {
        let route = |key, modifiers, repeat| route_key(key, modifiers, true, false, false, repeat);
        assert_eq!(
            route(Key::L, Modifiers::NONE, false),
            Some(WaveKey::Move(WaveStep::Frames(1)))
        );
        assert_eq!(
            route(Key::H, Modifiers::SHIFT, true),
            Some(WaveKey::Move(WaveStep::Samples(-48))),
            "held movement repeats"
        );
        assert_eq!(
            route(Key::CloseBracket, Modifiers::NONE, false),
            Some(WaveKey::Move(WaveStep::Samples(1)))
        );
        assert_eq!(route(Key::M, Modifiers::NONE, false), Some(WaveKey::Mute));
        assert_eq!(
            route(Key::M, Modifiers::NONE, true),
            None,
            "no repeated mute"
        );
        assert_eq!(
            route(Key::Equals, Modifiers::SHIFT, false),
            Some(WaveKey::Gain { louder: true })
        );
        assert_eq!(
            route(Key::Minus, Modifiers::NONE, false),
            Some(WaveKey::Gain { louder: false })
        );
        assert_eq!(route(Key::Z, Modifiers::COMMAND, false), None);
        assert_eq!(
            route_key(Key::Space, Modifiers::NONE, false, false, false, false),
            None,
            "a focused button keeps Space"
        );
        assert_eq!(
            route_key(Key::Escape, Modifiers::NONE, true, false, true, false),
            None,
            "IME owns input"
        );
    }
}
