//! Every command verb's argument schema, or why it has none yet.

use super::{
    ArgSchema, Constraint as C, Default as D, Kind as K, Names as N, Param, Selector as S, Variant,
    param as p, types::*,
};

const fn schema(
    selector: S,
    positional: &'static [Param],
    options: &'static [Param],
    examples: &'static [&'static str],
) -> ArgSchema {
    ArgSchema {
        selector,
        positional,
        options,
        forms: &[],
        rest_text: false,
        variants: &[],
        constraints: &[],
        examples,
    }
}

const PITCH_WORDS: &[&str] = &["preserve", "tape"];

// PAUSES

static HOLD: ArgSchema = schema(
    S::Cursor,
    &[p(
        "duration",
        K::Typed(&DURATION),
        D::Required,
        "pause length, rounded once to project frames",
    )],
    &[
        p(
            "video",
            K::Choice(&["freeze", "black", "ai"]),
            D::Value("freeze"),
            "picture: freeze the last frame, black, or freeze and request AI pictures",
        ),
        p(
            "audio",
            K::Choice(&["silence"]),
            D::Value("silence"),
            "a new pause is silent; choose room tone afterwards",
        ),
    ],
    &[
        ":hold 0.5s",
        ":hold 12f video=black",
        ":hold 1.5s video=ai audio=silence",
        ":hold 01:02.500 video=freeze",
    ],
);

static AI_HOLD: ArgSchema = schema(
    S::Cursor,
    &[p(
        "duration",
        K::Typed(&DURATION),
        D::Required,
        "pause length",
    )],
    &[],
    &[":ai-hold 0.5s"],
);

static HOLD_DURATION: ArgSchema = schema(
    S::Pause,
    &[p(
        "length",
        K::Typed(&FRAMES),
        D::Required,
        "the pause's new length in whole project frames",
    )],
    &[],
    &[":hold-duration 11f"],
);

static REVERSE: ArgSchema = schema(
    S::Cursor,
    &[p(
        "length",
        K::Typed(&DURATION),
        D::Value("8f"),
        "how much of the passage before the cursor plays backwards",
    )],
    &[],
    &[":reverse", ":reverse 8f", ":reverse 250ms"],
);

static PING_PONG: ArgSchema = schema(
    S::Cursor,
    &[p(
        "length",
        K::Typed(&DURATION),
        D::Value("12f"),
        "how much plays backwards, then forwards again",
    )],
    &[],
    &[":ping-pong", ":ping-pong 12f"],
);

static SPLIT_EDIT: ArgSchema = schema(
    S::Cursor,
    &[p(
        "length",
        K::Typed(&NONZERO_DURATION),
        D::Value("6f"),
        "how far the sound crosses the cut",
    )],
    &[],
    &[":jcut", ":jcut 6f", ":lcut 200ms"],
);

static HOLD_PROVIDER: ArgSchema = schema(
    S::Pause,
    &[p(
        "provider",
        K::Choice(&["ai", "fallback"]),
        D::Required,
        "request AI pictures, or return to the saved fallback",
    )],
    &[],
    &[":hold-provider ai", ":hold-provider fallback"],
);

static PICK_AI: ArgSchema = schema(
    S::Pause,
    &[p(
        "variant",
        K::Typed(&VARIANT),
        D::Required,
        "the AI variant to choose",
    )],
    &[],
    &[":pick-ai 2"],
);

static COMPARE_AI: ArgSchema = schema(
    S::Pause,
    &[p(
        "side",
        K::ChoiceOr(&["before"], &VARIANT),
        D::Means("toggles before and after"),
        "show the picture before the pause, or one variant",
    )],
    &[],
    &[":compare-ai", ":compare-ai before", ":compare-ai 2"],
);

// REPEAT & SPEED

static REPEAT_ARGS: ArgSchema = schema(
    S::RepeatOrBeat,
    &[p(
        "plays",
        K::Typed(&COUNT),
        D::Keeps,
        "total plays, including the first",
    )],
    &[
        p(
            "gap",
            K::Typed(&GAP),
            D::Keeps,
            "silent hold of each play's last picture between plays",
        ),
        p(
            "gap-step",
            K::Typed(&DURATION_STEP),
            D::Means("every gap the same"),
            "how much each later gap grows or shrinks; needs gap=",
        ),
        p(
            "speed-step",
            K::Typed(&SPEED_STEP),
            D::Keeps,
            "each later play faster or slower than the one before",
        ),
        p(
            "pitch",
            K::Choice(PITCH_WORDS),
            D::Keeps,
            "whether a speed step keeps its pitch or lets it follow speed",
        ),
        p(
            "gain-step",
            K::Typed(&GAIN_STEP),
            D::Keeps,
            "each later play louder or quieter",
        ),
        p(
            "zoom-step",
            K::Typed(&ZOOM_STEP),
            D::Keeps,
            "each later play closer",
        ),
        p(
            "progression",
            K::Choice(&["add", "multiply"]),
            D::Keeps,
            "whether named zoom and speed steps add or compound",
        ),
        p(
            "zoom-target",
            K::Choice(&["current", "center"]),
            D::Keeps,
            "zoom toward the saved target on the picture, or the middle",
        ),
        p(
            "role",
            K::Choice(&["audio", "video"]),
            D::Means("both roles, adding time"),
            "repeat one role of the Visual range without adding time",
        ),
        p(
            "overflow",
            K::Choice(&["trim"]),
            D::Means("refuses overflow"),
            "with role=: cut repeats at the last beat's end",
        ),
        p(
            "extend",
            K::Choice(&["hold"]),
            D::Means("refuses overflow"),
            "with role=audio: hold the last picture for the overflow",
        ),
    ],
    &[
        ":repeat 3",
        ":repeat 3 gap=6f gap-step=-2f",
        ":repeat 3 speed-step=10% pitch=tape progression=multiply",
        ":repeat 4 gain-step=3dB zoom-step=0.08 zoom-target=center",
        ":repeat gap=120ms",
        ":repeat 3 role=audio overflow=trim",
        ":repeat 2 role=audio extend=hold",
    ],
);

/// Parameters a role repeat refuses: it repeats one role in place, so it has
/// no gaps or per-play changes.
const ESCALATION: &[&str] = &[
    "gap",
    "gap-step",
    "speed-step",
    "pitch",
    "gain-step",
    "zoom-step",
    "progression",
    "zoom-target",
];

static REPEAT: ArgSchema = ArgSchema {
    constraints: &[
        C::Excludes(
            "role",
            ESCALATION,
            "a role repeat adds no gaps or per-play changes",
        ),
        C::Requires("gap-step", "gap"),
        C::Requires("overflow", "role"),
        C::Requires("extend", "role"),
        C::Excludes("overflow", &["extend"], "choose one overflow policy"),
    ],
    ..REPEAT_ARGS
};

static WRAP_REPEAT: ArgSchema = schema(
    S::WrapVisualOrBeat,
    &[p(
        "plays",
        K::Typed(&COUNT),
        D::Required,
        "total plays of the new Repeat",
    )],
    &[],
    &[":wrap-repeat 2"],
);

static RETIME: ArgSchema = schema(
    S::Beat,
    &[p(
        "speed",
        K::Typed(&SPEED),
        D::Required,
        "input speed: 0.5 is half speed",
    )],
    &[p(
        "pitch",
        K::ChoiceOr(PITCH_WORDS, &SEMITONES),
        D::Required,
        "keep pitch, let it follow speed (tape), or shift it",
    )],
    &[
        ":retime 0.75 pitch=preserve",
        ":retime 3/4 pitch=tape",
        ":retime 1 pitch=-2st",
    ],
);

static WRAP_RETIME: ArgSchema = ArgSchema {
    examples: &[":wrap-retime 0.5 pitch=preserve"],
    ..RETIME
};

static RAMP: ArgSchema = schema(
    S::Beat,
    &[
        p("from", K::Typed(&SPEED), D::Required, "speed at the start"),
        p("to", K::Typed(&SPEED), D::Required, "speed at the end"),
    ],
    &[
        p(
            "pitch",
            K::Choice(PITCH_WORDS),
            D::Value("tape; required with steps="),
            "keep pitch, or let it slide with speed",
        ),
        p(
            "steps",
            K::Typed(&RAMP_STEPS),
            D::Means("one continuous ramp"),
            "equal parts, each its own constant speed",
        ),
    ],
    &[
        ":ramp 1 0.25",
        ":ramp 1 2 pitch=preserve",
        ":ramp 1 0.25 steps=8 pitch=tape",
        ":ramp 3/4 2 steps=2 pitch=preserve",
    ],
);

static PITCH: ArgSchema = schema(
    S::Beat,
    &[p(
        "shift",
        K::Typed(&SEMITONES),
        D::Required,
        "0 removes the shift",
    )],
    &[],
    &[":pitch +5st", ":pitch -2", ":pitch 0"],
);

static SCOPE: ArgSchema = ArgSchema {
    variants: &[
        Variant {
            value: "play",
            positional: &[p(
                "play",
                K::Typed(&COUNT),
                D::Required,
                "one-based play number",
            )],
            options: &[],
        },
        Variant {
            value: "plays",
            positional: &[p(
                "plays",
                K::Typed(&PLAY_LIST),
                D::Required,
                "several one-based plays edited together",
            )],
            options: &[],
        },
    ],
    ..schema(
        S::Plays,
        &[p(
            "which",
            K::Choice(&["all", "play", "plays"]),
            D::Required,
            "every play, one play, or several plays",
        )],
        &[],
        &[
            ":scope all",
            ":scope play 2",
            ":scope plays 2-3",
            ":scope plays 1,3",
        ],
    )
};

// FRAMING

const TARGET_PARAM: Param = p(
    "target",
    K::Name(N::Target),
    D::Means("keeps the current center"),
    "where to zoom: a saved target, current, center, face:N or region:N",
);

static ZOOM: ArgSchema = ArgSchema {
    forms: &[("off", "returns to the full picture")],
    ..schema(
        S::Beat,
        &[p(
            "scale",
            K::Typed(&SCALE),
            D::Required,
            "how close: 2 is twice as close",
        )],
        &[
            TARGET_PARAM,
            p(
                "curve",
                K::Choice(&["step", "linear", "smoothstep", "smooth"]),
                D::Value("step"),
                "a step cut, or an eased creep",
            ),
        ],
        &[
            ":zoom 2",
            ":zoom 1.35 target=center curve=linear",
            ":zoom 2x curve=smoothstep",
            ":zoom off",
        ],
    )
};

static CREEP: ArgSchema = schema(
    S::Beat,
    &[],
    &[
        p(
            "from",
            K::Typed(&SCALE),
            D::Means("the current scale"),
            "scale at the start",
        ),
        p("to", K::Typed(&SCALE), D::Value("1.35"), "scale at the end"),
        TARGET_PARAM,
        p(
            "curve",
            K::Choice(&["smoothstep", "smooth", "linear"]),
            D::Value("smoothstep"),
            "how the creep eases",
        ),
    ],
    &[
        ":creep",
        ":creep from=1 to=1.4 target=current",
        ":creep to=2 curve=linear",
    ],
);

// SOUND OF A BEAT

static GAIN: ArgSchema = schema(
    S::VisualOrBeat,
    &[p(
        "level",
        K::Typed(&GAIN_LEVEL),
        D::Means("opens the gain draft"),
        "clip gain",
    )],
    &[p(
        "range",
        K::Typed(&FRAME_RANGE),
        D::Means("the whole beat"),
        "a constant step over these local frames of the selected beat",
    )],
    &[
        ":gain",
        ":gain +3dB",
        ":gain -6",
        ":gain +=3dB",
        ":gain -=1.5dB",
        ":gain +3dB range=4-10",
    ],
);

static SATURATE: ArgSchema = ArgSchema {
    forms: &[("off", "removes the saturation stage")],
    ..schema(
        S::Beat,
        &[p(
            "drive",
            K::Typed(&DRIVE),
            D::Required,
            "soft-clipper drive",
        )],
        &[],
        &[":saturate 12dB", ":saturate off"],
    )
};

static BALANCE_ARGS: ArgSchema = schema(
    S::Beat,
    &[p(
        "balance",
        K::ChoiceOr(&["left", "right", "center", "off"], &BALANCE),
        D::Required,
        "stereo balance; the opposite channel is attenuated",
    )],
    &[],
    &[
        ":balance -50%",
        ":balance +50%",
        ":balance left",
        ":balance center",
    ],
);

static AUDIO_LAG: ArgSchema = schema(
    S::Beat,
    &[p(
        "offset",
        K::Typed(&SIGNED_DURATION),
        D::Required,
        "sound later (+) or earlier (-) than its picture; 0 realigns",
    )],
    &[],
    &[
        ":audio-lag 3f",
        ":audio-lag +80ms",
        ":audio-lag -2f",
        ":audio-lag 0",
    ],
);

static EDGE: ArgSchema = schema(
    S::Beat,
    &[
        p(
            "policy",
            K::Choice(&["hard", "auto"]),
            D::Required,
            "a hard cut, or the automatic short fade",
        ),
        p(
            "side",
            K::Choice(&["both", "start", "end", "plays", "gaps"]),
            D::Value("both"),
            "which edges: a Repeat's loop seams (plays) or gap edges (gaps)",
        ),
    ],
    &[],
    &[
        ":edge hard",
        ":edge auto start",
        ":edge hard end",
        ":edge hard plays",
        ":edge auto gaps",
    ],
);

static SOUND_MUTE: ArgSchema = schema(
    S::Sound,
    &[p(
        "muted",
        K::Choice(&["on", "off"]),
        D::Required,
        "mute or unmute",
    )],
    &[],
    &[":sound-mute on", ":sound-mute off"],
);

static SOUND_CHANNELS: ArgSchema = schema(
    S::Project,
    &[p(
        "layout",
        K::Choice(&["mono", "stereo", "none"]),
        D::Required,
        "how the next imported sound without a speaker layout is heard; none refuses it",
    )],
    &[],
    &[
        ":sound-channels mono",
        ":sound-channels stereo",
        ":sound-channels none",
    ],
);

// GAGS, CAPTIONS & CUTAWAYS

static CAPTION: ArgSchema = ArgSchema {
    forms: &[("clear", "removes captions there")],
    rest_text: true,
    ..schema(
        S::VisualOrBeat,
        &[p(
            "text",
            K::Text,
            D::Required,
            "the caption; \"quotes\" keep words like at= in it",
        )],
        &[
            p(
                "at",
                K::Choice(&["bottom", "top", "center", "middle"]),
                D::Value("bottom"),
                "where the caption sits",
            ),
            p(
                "delay",
                K::Typed(&DURATION),
                D::Means("starts with the beat"),
                "starts it later in the beat",
            ),
            p(
                "reveal",
                K::Typed(&COUNT),
                D::Means("every play"),
                "the first Repeat play that shows it",
            ),
        ],
        &[
            ":caption Are we done?",
            ":caption Are we done? at=top delay=4f",
            ":caption \"at=noon\" reveal=3",
            ":caption clear",
        ],
    )
};

static CUTAWAY: ArgSchema = ArgSchema {
    forms: &[("clear", "removes the cutaway")],
    ..schema(
        S::Beat,
        &[],
        &[
            p(
                "register",
                K::Name(N::Register),
                D::Means("the unnamed register"),
                "the copied pictures shown over the beat",
            ),
            p(
                "fit",
                K::Choice(&["hold", "loop", "bounce", "gap"]),
                D::Value("hold"),
                "what plays once the copy is shorter than the beat",
            ),
            p(
                "audio",
                K::Choice(&["keep"]),
                D::Value("keep"),
                "a cutaway replaces picture only",
            ),
        ],
        &[
            ":cutaway",
            ":cutaway register=a",
            ":cutaway register=a fit=loop audio=keep",
            ":cutaway clear",
        ],
    )
};

const GAG_PAUSE: Param = p(
    "pause",
    K::Typed(&DURATION),
    D::Value("1.5s"),
    "the pause length",
);
const GAG_REGISTER: Param = p(
    "register",
    K::Name(N::Register),
    D::Means("the unnamed register"),
    "the copied beat it uses",
);

const GAG_VARIANTS: &[Variant] = &[
    Variant {
        value: "long-answer",
        positional: &[],
        options: &[
            GAG_PAUSE,
            p(
                "creep",
                K::Typed(&DECIMAL),
                D::Value("1.35"),
                "the creep's final scale",
            ),
        ],
    },
    Variant {
        value: "escalator",
        positional: &[],
        options: &[
            p("plays", K::Typed(&COUNT), D::Value("3"), "total plays"),
            p(
                "gain-step",
                K::Typed(&GAIN_STEP),
                D::Value("3dB"),
                "each later play louder",
            ),
            p(
                "zoom-step",
                K::Typed(&ZOOM_STEP),
                D::Value("0.08"),
                "each later play closer",
            ),
        ],
    },
    Variant {
        value: "non-sequitur",
        positional: &[],
        options: &[GAG_REGISTER],
    },
    Variant {
        value: "one-more-time",
        positional: &[],
        options: &[
            p(
                "plays",
                K::Typed(&TWO_OR_MORE),
                D::Value("3"),
                "total plays",
            ),
            p(
                "gap",
                K::Typed(&DURATION),
                D::Value("0.5s"),
                "the first gap",
            ),
            p(
                "shorten",
                K::Typed(&DURATION),
                D::Value("200ms"),
                "each later gap shorter by",
            ),
            p(
                "vary",
                K::Typed(&VARY),
                D::Means("no variation"),
                "random variation of each gap",
            ),
            p(
                "seed",
                K::Typed(&SEED),
                D::Means("a fresh pinned seed"),
                "the variation's seed; needs vary=",
            ),
        ],
    },
    Variant {
        value: "nothing-happens",
        positional: &[],
        options: &[
            GAG_REGISTER,
            p(
                "tone",
                K::Typed(&DURATION),
                D::Value("1s"),
                "the tone's length",
            ),
            p(
                "silence",
                K::Typed(&DURATION),
                D::Value("1s"),
                "the silence after it",
            ),
        ],
    },
    Variant {
        value: "are-we-done",
        positional: &[],
        options: &[GAG_REGISTER, GAG_PAUSE],
    },
];

const GAG_NAME: Param = p(
    "gag",
    K::Name(N::Gag),
    D::Required,
    "a built-in recipe or a saved preset",
);

static GAG: ArgSchema = ArgSchema {
    variants: GAG_VARIANTS,
    ..schema(
        S::BeatOrCursor,
        &[GAG_NAME],
        &[],
        &[
            ":gag long-answer",
            ":gag long-answer pause=2s creep=1.5",
            ":gag escalator plays=4 gain-step=-1.5dB zoom-step=0.08",
            ":gag non-sequitur register=r",
            ":gag one-more-time plays=3 gap=500ms shorten=200ms vary=20% seed=7",
            ":gag nothing-happens register=r tone=1s silence=1s",
            ":gag are-we-done register=r pause=1.5s",
        ],
    )
};

static GAG_INSPECT: ArgSchema = ArgSchema {
    selector: S::View,
    examples: &[":gag-inspect long-answer", ":gag-inspect escalator plays=5"],
    ..GAG
};

static GAG_SET: ArgSchema = schema(
    S::Gag,
    &[],
    &[
        p(
            "pause",
            K::Typed(&DURATION),
            D::Keeps,
            "long-answer, are-we-done: the pause",
        ),
        p(
            "creep",
            K::Typed(&DECIMAL),
            D::Keeps,
            "long-answer: the final scale",
        ),
        p(
            "plays",
            K::Typed(&COUNT),
            D::Keeps,
            "escalator, one-more-time: total plays",
        ),
        p(
            "gain-step",
            K::Typed(&GAIN_STEP),
            D::Keeps,
            "escalator: each later play louder",
        ),
        p(
            "zoom-step",
            K::Typed(&ZOOM_STEP),
            D::Keeps,
            "escalator: each later play closer",
        ),
        p(
            "gap",
            K::Typed(&DURATION),
            D::Keeps,
            "one-more-time: the first gap",
        ),
        p(
            "shorten",
            K::Typed(&DURATION),
            D::Keeps,
            "one-more-time: each later gap shorter by",
        ),
        p(
            "vary",
            K::Typed(&VARY),
            D::Keeps,
            "one-more-time: random gap variation",
        ),
        p(
            "seed",
            K::Typed(&SEED),
            D::Keeps,
            "one-more-time: the variation's seed",
        ),
        p(
            "tone",
            K::Typed(&DURATION),
            D::Keeps,
            "nothing-happens: the tone",
        ),
        p(
            "silence",
            K::Typed(&DURATION),
            D::Keeps,
            "nothing-happens: the silence",
        ),
        p(
            "register",
            K::Name(N::Register),
            D::Keeps,
            "the copied beat the gag uses",
        ),
    ],
    &[],
);

static GAG_SAVE: ArgSchema = schema(
    S::Gag,
    &[p(
        "name",
        K::Text,
        D::Required,
        "one-word preset name, kept for every project",
    )],
    &[],
    &[":gag-save answer"],
);

// STRUCTURE, TRIM & SLIP

static SLIP: ArgSchema = schema(
    S::SourceBeat,
    &[p(
        "amount",
        K::Typed(&SIGNED_FRAMES),
        D::Required,
        "moves the picture and sound inside the beat",
    )],
    &[],
    &[":slip +5f", ":slip -3f"],
);

static ROLL: ArgSchema = schema(
    S::SourceBeat,
    &[p(
        "amount",
        K::Typed(&SIGNED_FRAMES),
        D::Required,
        "moves the cut after the beat, keeping both beats' total length",
    )],
    &[],
    &[":roll +2f", ":roll -3f"],
);

static TRIM: ArgSchema = schema(
    S::SourceBeat,
    &[],
    &[
        p(
            "edge",
            K::Choice(&["in", "out", "slip", "roll"]),
            D::Means("opens Trim at zero; give all three or none"),
            "which value the draft adjusts",
        ),
        p(
            "delta",
            K::Typed(&TRIM_FRAMES),
            D::Means("opens Trim at zero; give all three or none"),
            "the amount",
        ),
        p(
            "mode",
            K::Choice(&["ripple", "overwrite"]),
            D::Means("opens Trim at zero; give all three or none"),
            "whether later beats move",
        ),
    ],
    &[
        ":trim",
        ":trim edge=out delta=-3f mode=ripple",
        ":trim mode=overwrite delta=+5f edge=in",
    ],
);

static DELETE: ArgSchema = schema(
    S::VisualOrBeat,
    &[],
    &[p(
        "role",
        K::Choice(&["audio", "video"]),
        D::Means("deletes linked time"),
        "delete one role over the Visual range",
    )],
    &[":delete", ":delete role=audio", ":delete role=video"],
);

static DELETE_FRAMES: ArgSchema = schema(
    S::Cursor,
    &[p(
        "frames",
        K::Typed(&FRAMES),
        D::Value("1f"),
        "whole frames after the cursor",
    )],
    &[],
    &[":delete-frames", ":delete-frames 12f"],
);

// REGISTERS, MARKS & MACROS

const fn letter(names: N, help: &'static str) -> Param {
    p("name", K::Name(names), D::Required, help)
}

static MARK: ArgSchema = schema(
    S::TimelineCursor,
    &[letter(N::Mark, "the mark's letter")],
    &[],
    &[":mark a", ":jump a", ":unmark a"],
);

static REGISTER: ArgSchema = schema(
    S::NextRegister,
    &[letter(N::Register, "a–z, or \" for the unnamed register")],
    &[],
    &[":register a", ":register \""],
);

static RECIPE_SAVE: ArgSchema = schema(
    S::Beat,
    &[letter(
        N::Register,
        "the register the group's recipe is kept in",
    )],
    &[],
    &[":recipe-save a"],
);

static RECIPE: ArgSchema = schema(
    S::Cursor,
    &[letter(N::Recipe, "the local recipe to insert")],
    &[],
    &[":recipe a"],
);

static RECIPE_INSPECT: ArgSchema = schema(
    S::View,
    &[letter(N::Recipe, "the local recipe to list")],
    &[],
    &[":recipe-inspect a"],
);

static FRAMING_SAVE: ArgSchema = schema(
    S::Beat,
    &[letter(N::Register, "the register the framing is kept in")],
    &[],
    &[":framing-save a"],
);

static RECORD: ArgSchema = schema(
    S::Macro,
    &[letter(
        N::Register,
        "the register the macro is recorded into",
    )],
    &[],
    &[":record a"],
);

static MACRO: ArgSchema = schema(
    S::Macro,
    &[
        letter(N::Macro, "the recorded macro to run"),
        p(
            "count",
            K::Typed(&COUNT),
            D::Value("1"),
            "how many times to run it",
        ),
    ],
    &[],
    &[":macro a", ":macro q 3"],
);

// PROJECT & VIEW

static MONITOR_ARGS: ArgSchema = schema(
    S::View,
    &[p(
        "volume",
        K::Typed(&MONITOR),
        D::Required,
        "monitoring volume, independent of export gain",
    )],
    &[],
    &[":monitor 25%", ":monitor 100"],
);

static PROXIES: ArgSchema = schema(
    S::Project,
    &[p(
        "state",
        K::Choice(&["on", "off", "retry"]),
        D::Required,
        "automatic seek proxies",
    )],
    &[],
    &[":proxies on", ":proxies off", ":proxies retry"],
);

static TIMELINE: ArgSchema = schema(
    S::View,
    &[p(
        "view",
        K::Choice(&["story", "timing", "detail"]),
        D::Required,
        "cards per beat, beats at their length, or with detail",
    )],
    &[],
    &[":timeline story", ":timeline timing", ":timeline detail"],
);

static AUDITION_CONTEXT: ArgSchema = schema(
    S::View,
    &[],
    &[
        p(
            "lead",
            K::Typed(&DURATION),
            D::Required,
            "heard before a selection",
        ),
        p("follow", K::Typed(&DURATION), D::Required, "heard after it"),
    ],
    &[":audition-context lead=500ms follow=750ms"],
);

static TARGET_RENAME: ArgSchema = ArgSchema {
    rest_text: true,
    ..schema(
        S::Project,
        &[
            p(
                "target",
                K::Name(N::Target),
                D::Required,
                "the saved target, by id or label",
            ),
            p("label", K::Text, D::Required, "its new label"),
        ],
        &[],
        &[":target-rename target-1 Speaker on the left"],
    )
};

static TARGET_DELETE: ArgSchema = schema(
    S::Project,
    &[p(
        "target",
        K::Name(N::Target),
        D::Required,
        "a saved target nothing uses, by id or one-word label",
    )],
    &[],
    &[":target-delete target-1"],
);

// SELECTION AND NAMES

/// One selector form at a time: they choose different things to select.
const SELECTORS: &[&str] = &["role", "node", "nodes", "ranges", "search", "start", "end"];
const ONE_SELECTOR: &str = "choose one way to select";

static SELECT: ArgSchema = ArgSchema {
    variants: &[Variant {
        value: "marks",
        positional: &[],
        options: &[
            p(
                "start",
                K::Typed(&MARK_ID),
                D::Required,
                "the mark the range starts at",
            ),
            p(
                "end",
                K::Typed(&MARK_ID),
                D::Required,
                "the mark the range ends at",
            ),
        ],
    }],
    constraints: &[
        C::Excludes("role", &without(SELECTORS, 0), ONE_SELECTOR),
        C::Excludes("node", &without(SELECTORS, 1), ONE_SELECTOR),
        C::Excludes("nodes", &without(SELECTORS, 2), ONE_SELECTOR),
        C::Excludes("ranges", &without(SELECTORS, 3), ONE_SELECTOR),
        C::Excludes("search", &without(SELECTORS, 4), ONE_SELECTOR),
    ],
    ..schema(
        S::TimelineCursor,
        &[p(
            "form",
            K::Choice(&["marks"]),
            D::Means("selects the Original moment, or by the parameter given"),
            "marks selects Your edit between two marks",
        )],
        &[
            p(
                "role",
                K::Choice(&["audio", "video", "linked"]),
                D::Means("linked"),
                "the media role the next Visual selection covers",
            ),
            p(
                "node",
                K::Typed(&NODE_ID),
                D::Means("no node"),
                "select one beat by its exact node id",
            ),
            p(
                "nodes",
                K::Typed(&NODE_LIST),
                D::Means("no node set"),
                "select several beats by exact node id",
            ),
            p(
                "ranges",
                K::Typed(&RANGE_LIST),
                D::Means("no ranges"),
                "select frame ranges relative to the Edit cursor",
            ),
            p(
                "search",
                K::Typed(&SEARCH_TEXT),
                D::Means("no search"),
                "select where the transcript says these words",
            ),
        ],
        &[
            ":select",
            ":select role=audio",
            ":select marks start=native-mark-a end=native-mark-b",
            ":select node=one",
            ":select nodes=[\"id-a\",\"id-b\"]",
            ":select ranges=[{\"start\":2,\"end\":6}]",
            ":select search=\"hello world\"",
        ],
    )
};

/// Every selector but one, for the rules that keep them exclusive.
const fn without(all: &[&'static str], skip: usize) -> [&'static str; 6] {
    let mut out = [""; 6];
    let mut from = 0;
    let mut to = 0;
    while from < all.len() {
        if from != skip {
            out[to] = all[from];
            to += 1;
        }
        from += 1;
    }
    out
}

static GROUP: ArgSchema = schema(
    S::VisualOrBeat,
    &[],
    &[p(
        "name",
        K::Typed(&GROUP_NAME),
        D::Required,
        "the group's name, JSON-quoted so it may hold spaces",
    )],
    &[":group name=\"the answer\""],
);

// ORIGINAL MOMENTS

static MOMENT: ArgSchema = ArgSchema {
    rest_text: true,
    ..schema(
        S::TimelineCursor,
        &[p(
            "name",
            K::Text,
            D::Required,
            "the selected Original moment's name, spaces allowed",
        )],
        &[p(
            "tags",
            K::Typed(&TAG_LIST),
            D::Means("no tags"),
            "comma-separated tags",
        )],
        &[
            ":moment Big laugh",
            ":moment Big laugh tags=reaction,breath",
        ],
    )
};

static MOMENTS: ArgSchema = ArgSchema {
    rest_text: true,
    ..schema(
        S::View,
        &[p(
            "filter",
            K::Text,
            D::Means("every moment"),
            "words that match names and tags loosely; tag:reaction keeps one tag",
        )],
        &[],
        &[":moments", ":moments tag:reaction"],
    )
};

static MOMENT_RENAME: ArgSchema = ArgSchema {
    rest_text: true,
    ..schema(
        S::TimelineCursor,
        &[p(
            "name",
            K::Text,
            D::Required,
            "the selected moment's new name, spaces allowed",
        )],
        &[],
        &[":moment-rename Big laugh"],
    )
};

static MOMENT_TAGS: ArgSchema = ArgSchema {
    forms: &[("none", "clears the selected moment's tags")],
    ..schema(
        S::TimelineCursor,
        &[p(
            "tags",
            K::Typed(&TAG_LIST),
            D::Required,
            "the selected moment's tags, replacing its current ones",
        )],
        &[],
        &[":moment-tags reaction,breath", ":moment-tags none"],
    )
};

// TARGETS AND AI

static TRACK: ArgSchema = ArgSchema {
    rest_text: true,
    ..schema(
        S::Project,
        &[p(
            "target",
            K::Name(N::Target),
            D::Means("the chosen target"),
            "the saved target to track; end with through-shots to cross cuts",
        )],
        &[],
        &[":track", ":track target-1", ":track target-1 through-shots"],
    )
};

static GENERATE: ArgSchema = schema(
    S::Pause,
    &[p(
        "variants",
        K::Typed(&VARIANTS),
        D::Value("1"),
        "how many variants, each from a different seed; before the parameters",
    )],
    &[
        p(
            "mode",
            K::Choice(&["auto", "bridge", "extend-left", "extend-right"]),
            D::Means("keeps the mode, or auto"),
            "which neighbouring pictures condition the pause",
        ),
        p(
            "motion",
            K::Choice(&["still", "subtle", "moderate"]),
            D::Means("keeps the motion, or still"),
            "how much the pictures should move",
        ),
        p(
            "target",
            K::ChoiceOr(&["none"], &TARGET_ID),
            D::Means("keeps the captured target"),
            "a saved target for region checks; none clears it",
        ),
        p(
            "text",
            K::Rest,
            D::Means("keeps the guidance, or none"),
            "guidance for the model, up to 512 bytes; always last",
        ),
    ],
    &[
        ":generate",
        ":generate 3",
        ":generate 2 mode=bridge motion=subtle target=none",
        ":generate mode=auto motion=still text=Keep the hands still.",
        ":generate target=target-1",
    ],
);

static GENERATE_AI: ArgSchema = ArgSchema {
    examples: &[":generate-ai 2 mode=extend-left"],
    ..GENERATE
};

// AUDIO TREATMENTS

static EQ: ArgSchema = ArgSchema {
    forms: &[("off", "removes the equalizer")],
    ..schema(
        S::Beat,
        &[],
        &[
            p(
                "bass",
                K::Typed(&EQ_BAND),
                D::Required,
                "low shelf level; all three bands together",
            ),
            p(
                "middle",
                K::Typed(&EQ_BAND),
                D::Required,
                "middle band level",
            ),
            p(
                "treble",
                K::Typed(&EQ_BAND),
                D::Required,
                "high shelf level",
            ),
            p(
                "low-cut",
                K::ChoiceOr(&["off"], &EQ_CUT),
                D::Value("off"),
                "high-pass cutoff",
            ),
            p(
                "high-cut",
                K::ChoiceOr(&["off"], &EQ_CUT),
                D::Value("off"),
                "low-pass cutoff, above the low cut",
            ),
        ],
        &[
            ":eq",
            ":eq bass=3dB middle=-6dB treble=0dB",
            ":eq bass=3dB middle=0dB treble=0dB low-cut=80Hz high-cut=12000Hz",
            ":eq bass=0dB middle=0dB treble=0dB low-cut=off",
            ":eq off",
        ],
    )
};

static SOUND_EQ: ArgSchema = ArgSchema {
    selector: S::Sound,
    examples: &[
        ":sound-eq bass=3dB middle=0dB treble=0dB",
        ":sound-eq bass=0dB middle=0dB treble=0dB low-cut=80Hz high-cut=12000Hz",
        ":sound-eq off",
    ],
    ..EQ
};

static SOUND_ENVELOPE: ArgSchema = ArgSchema {
    forms: &[("clear", "removes the sound's gain envelopes")],
    ..schema(
        S::Sound,
        &[
            p(
                "start",
                K::Typed(&ENVELOPE_FRAME),
                D::Required,
                "where the ramp starts, from the sound's onset",
            ),
            p(
                "end",
                K::Typed(&ENVELOPE_FRAME),
                D::Required,
                "where the ramp ends; a matching range is replaced",
            ),
            p(
                "from",
                K::Typed(&ENVELOPE_LEVEL),
                D::Required,
                "the level at the start",
            ),
            p(
                "to",
                K::Typed(&ENVELOPE_LEVEL),
                D::Required,
                "the level at the end",
            ),
            p(
                "curve",
                K::Choice(&["step", "linear", "smoothstep"]),
                D::Required,
                "the ramp's shape",
            ),
        ],
        &[],
        &[
            ":sound-envelope 0f 30f -12dB 0dB linear",
            ":sound-envelope 4f 8f 0dB -6dB smoothstep",
            ":sound-envelope clear",
        ],
    )
};

static SOURCE_INTERPRET: ArgSchema = ArgSchema {
    forms: &[("reset", "returns to the measured interpretation")],
    ..schema(
        S::Project,
        &[],
        &[
            p(
                "range",
                K::Choice(&["auto", "limited", "full"]),
                D::Keeps,
                "the Original's color range",
            ),
            p(
                "chroma",
                K::Choice(&[
                    "auto",
                    "left",
                    "center",
                    "top-left",
                    "top",
                    "bottom-left",
                    "bottom",
                ]),
                D::Keeps,
                "where chroma samples sit",
            ),
        ],
        &[
            ":source-interpret",
            ":source-interpret range=full chroma=left",
            ":source-interpret reset",
        ],
    )
};

static BLEEP: ArgSchema = schema(
    S::VisualOrBeat,
    &[p(
        "frequency",
        K::Typed(&HERTZ),
        D::Value("1000Hz"),
        "the tone's frequency",
    )],
    &[p(
        "level",
        K::Typed(&BLEEP_LEVEL),
        D::Value("-10dB"),
        "the tone's level",
    )],
    &[":bleep", ":bleep 880Hz", ":bleep 880Hz level=-6dB"],
);

static TAIL: ArgSchema = schema(
    S::PauseOrCursor,
    &[p(
        "length",
        K::Typed(&DURATION),
        D::Means("rings for the whole selected pause"),
        "how long it rings, or the new pause's length",
    )],
    &[
        p(
            "effect",
            K::Choice(&["reverb", "delay"]),
            D::Value("reverb"),
            "reverb, or a 300 ms echo",
        ),
        p(
            "input",
            K::Choice(&["original", "sends"]),
            D::Value("original"),
            "the processed Original, or only matching tail sends",
        ),
    ],
    &[
        ":tail",
        ":tail 1s",
        ":tail 1s effect=delay",
        ":tail effect=reverb input=sends",
    ],
);

static SEND: [Param; 2] = [
    p(
        "effect",
        K::Choice(&["reverb", "delay"]),
        D::Required,
        "reverb, or a 300 ms echo; each has its own level",
    ),
    p(
        "level",
        K::ChoiceOr(&["off"], &SEND_LEVEL),
        D::Required,
        "the return level; off removes it",
    ),
];

static WET_SEND: ArgSchema = schema(
    S::Beat,
    &SEND,
    &[],
    &[":wet-send reverb -6dB", ":wet-send delay off"],
);

static TAIL_SEND: ArgSchema = schema(
    S::Beat,
    &SEND,
    &[],
    &[":tail-send reverb -6dB", ":tail-send delay off"],
);

static SOUND_WET_SEND: ArgSchema = schema(
    S::Sound,
    &SEND,
    &[],
    &[":sound-wet-send reverb -6dB", ":sound-wet-send delay off"],
);

static SOUND_TAIL_SEND: ArgSchema = schema(
    S::Sound,
    &SEND,
    &[],
    &[
        ":sound-tail-send delay -12dB",
        ":sound-tail-send reverb off",
    ],
);

static SOUND_SATURATE: ArgSchema = ArgSchema {
    selector: S::Sound,
    examples: &[":sound-saturate 6dB", ":sound-saturate off"],
    ..SATURATE
};

static SOUND_BALANCE: ArgSchema = ArgSchema {
    selector: S::Sound,
    examples: &[":sound-balance -50%", ":sound-balance center"],
    ..BALANCE_ARGS
};

static AUDIO_SHIFT: ArgSchema = ArgSchema {
    constraints: &[C::Excludes(
        "overflow",
        &["extend"],
        "choose one overflow policy",
    )],
    ..schema(
        S::Cursor,
        &[p(
            "frames",
            K::Typed(&SHIFT_FRAMES),
            D::Required,
            "how far sound slides, later (+) or earlier (-)",
        )],
        &[
            p(
                "overflow",
                K::Choice(&["trim"]),
                D::Means("refuses overflow"),
                "cut sound pushed past the group's end",
            ),
            p(
                "extend",
                K::Choice(&["hold"]),
                D::Means("refuses overflow"),
                "hold the last picture for sound pushed past the end",
            ),
        ],
        &[
            ":audio-shift +5f",
            ":audio-shift -3f overflow=trim",
            ":audio-shift +5f extend=hold",
        ],
    )
};

static SOUND_AT: ArgSchema = schema(
    S::Sound,
    &[p(
        "sample",
        K::Typed(&SAMPLE),
        D::Required,
        "the sound's exact start on the 48 kHz clock",
    )],
    &[],
    &[":sound-at 274000"],
);

static SOUND_GAIN: ArgSchema = schema(
    S::Sound,
    &[p(
        "level",
        K::Typed(&SOUND_LEVEL),
        D::Required,
        "the placed sound's gain in dB",
    )],
    &[],
    &[":sound-gain -3", ":sound-gain 1.5"],
);

static SOUND_EDGES: ArgSchema = schema(
    S::Sound,
    &[p(
        "edges",
        K::Choice(&["soft", "hard"]),
        D::Required,
        "short fades at both ends, or none",
    )],
    &[],
    &[":sound-edges soft", ":sound-edges hard"],
);

/// Every migrated verb. The parser executes these through its own typed
/// code; tests run each schema's examples, parameter names and choices
/// through it.
pub static SCHEMAS: &[(&str, &ArgSchema)] = &[
    ("hold", &HOLD),
    ("ai-hold", &AI_HOLD),
    ("hold-duration", &HOLD_DURATION),
    ("reverse", &REVERSE),
    ("ping-pong", &PING_PONG),
    ("jcut", &SPLIT_EDIT),
    ("lcut", &SPLIT_EDIT),
    ("hold-provider", &HOLD_PROVIDER),
    ("pick-ai", &PICK_AI),
    ("compare-ai", &COMPARE_AI),
    ("repeat", &REPEAT),
    ("wrap-repeat", &WRAP_REPEAT),
    ("retime", &RETIME),
    ("wrap-retime", &WRAP_RETIME),
    ("ramp", &RAMP),
    ("pitch", &PITCH),
    ("scope", &SCOPE),
    ("zoom", &ZOOM),
    ("creep", &CREEP),
    ("gain", &GAIN),
    ("saturate", &SATURATE),
    ("balance", &BALANCE_ARGS),
    ("audio-lag", &AUDIO_LAG),
    ("edge", &EDGE),
    ("sound-mute", &SOUND_MUTE),
    ("sound-channels", &SOUND_CHANNELS),
    ("caption", &CAPTION),
    ("cutaway", &CUTAWAY),
    ("gag", &GAG),
    ("gag-inspect", &GAG_INSPECT),
    ("gag-set", &GAG_SET),
    ("gag-save", &GAG_SAVE),
    ("slip", &SLIP),
    ("roll", &ROLL),
    ("trim", &TRIM),
    ("delete", &DELETE),
    ("delete-frames", &DELETE_FRAMES),
    ("mark", &MARK),
    ("jump", &MARK),
    ("unmark", &MARK),
    ("register", &REGISTER),
    ("recipe-save", &RECIPE_SAVE),
    ("recipe", &RECIPE),
    ("recipe-inspect", &RECIPE_INSPECT),
    ("framing-save", &FRAMING_SAVE),
    ("record", &RECORD),
    ("macro", &MACRO),
    ("monitor", &MONITOR_ARGS),
    ("proxies", &PROXIES),
    ("timeline", &TIMELINE),
    ("audition-context", &AUDITION_CONTEXT),
    ("target-rename", &TARGET_RENAME),
    ("target-delete", &TARGET_DELETE),
    ("select", &SELECT),
    ("group", &GROUP),
    ("moment", &MOMENT),
    ("moments", &MOMENTS),
    ("moment-rename", &MOMENT_RENAME),
    ("moment-tags", &MOMENT_TAGS),
    ("track", &TRACK),
    ("generate", &GENERATE),
    ("generate-ai", &GENERATE_AI),
    ("eq", &EQ),
    ("sound-eq", &SOUND_EQ),
    ("sound-envelope", &SOUND_ENVELOPE),
    ("source-interpret", &SOURCE_INTERPRET),
    ("bleep", &BLEEP),
    ("tail", &TAIL),
    ("wet-send", &WET_SEND),
    ("tail-send", &TAIL_SEND),
    ("sound-wet-send", &SOUND_WET_SEND),
    ("sound-tail-send", &SOUND_TAIL_SEND),
    ("sound-saturate", &SOUND_SATURATE),
    ("sound-balance", &SOUND_BALANCE),
    ("audio-shift", &AUDIO_SHIFT),
    ("sound-at", &SOUND_AT),
    ("sound-gain", &SOUND_GAIN),
    ("sound-edges", &SOUND_EDGES),
    // No arguments.
    ("split", &ArgSchema::none(S::BeatAtCursor)),
    ("room-tone", &ArgSchema::none(S::Pause)),
    ("hold-silence", &ArgSchema::none(S::Pause)),
    ("isolate-gap", &ArgSchema::none(S::Plays)),
    ("lift", &ArgSchema::none(S::VisualOrBeat)),
    ("duplicate", &ArgSchema::none(S::VisualOrBeat)),
    ("explode", &ArgSchema::none(S::Beat)),
    ("ungroup", &ArgSchema::none(S::Beat)),
    ("enter", &ArgSchema::none(S::Beat)),
    ("parent", &ArgSchema::none(S::View)),
    ("insert", &ArgSchema::none(S::Cursor)),
    ("splice", &ArgSchema::none(S::Cursor)),
    ("paste", &ArgSchema::none(S::Cursor)),
    ("paste-before", &ArgSchema::none(S::Cursor)),
    ("yank", &ArgSchema::none(S::VisualOrBeat)),
    ("gain-mute", &ArgSchema::none(S::Beat)),
    ("undo", &ArgSchema::none(S::Project)),
    ("redo", &ArgSchema::none(S::Project)),
    ("jump-back", &ArgSchema::none(S::View)),
    ("jump-forward", &ArgSchema::none(S::View)),
    ("marks", &ArgSchema::none(S::View)),
    ("play", &ArgSchema::none(S::TimelineCursor)),
    ("audition", &ArgSchema::none(S::VisualOrBeat)),
    ("source", &ArgSchema::none(S::View)),
    ("sequence", &ArgSchema::none(S::View)),
    ("help", &ArgSchema::none(S::View)),
    ("registers", &ArgSchema::none(S::View)),
    ("record-stop", &ArgSchema::none(S::Macro)),
    ("record-cancel", &ArgSchema::none(S::Macro)),
    ("gag-presets", &ArgSchema::none(S::View)),
    ("track-cancel", &ArgSchema::none(S::Project)),
    ("sting", &ArgSchema::none(S::Project)),
    ("import", &ArgSchema::none(S::Project)),
    ("sound-place", &ArgSchema::none(S::Cursor)),
    ("sounds", &ArgSchema::none(S::View)),
    ("sound-cut", &ArgSchema::none(S::Sound)),
    ("sound-attach", &ArgSchema::none(S::Cursor)),
    ("sound-detach", &ArgSchema::none(S::Beat)),
    ("sound-delete", &ArgSchema::none(S::Sound)),
    ("sound-allow", &ArgSchema::none(S::Sound)),
    ("sound-silence", &ArgSchema::none(S::Sound)),
    ("revert-ai", &ArgSchema::none(S::Pause)),
    ("cancel-ai", &ArgSchema::none(S::Pause)),
    ("next-ai", &ArgSchema::none(S::Pause)),
    ("prev-ai", &ArgSchema::none(S::Pause)),
    ("previous-ai", &ArgSchema::none(S::Pause)),
    ("preview-ai", &ArgSchema::none(S::Pause)),
    ("audition-ai", &ArgSchema::none(S::Pause)),
    ("accept-ai", &ArgSchema::none(S::Pause)),
    ("discard-ai", &ArgSchema::none(S::Pause)),
    ("keep-ai", &ArgSchema::none(S::Pause)),
    ("render", &ArgSchema::none(S::Project)),
    ("takes", &ArgSchema::none(S::View)),
    ("renders", &ArgSchema::none(S::View)),
    ("relink", &ArgSchema::none(S::Project)),
    ("sound-relink", &ArgSchema::none(S::Project)),
    ("recovery", &ArgSchema::none(S::View)),
    ("correct", &ArgSchema::none(S::View)),
    ("jobs", &ArgSchema::none(S::View)),
    ("storage", &ArgSchema::none(S::View)),
    ("backups", &ArgSchema::none(S::View)),
    ("portable-copy", &ArgSchema::none(S::Project)),
    ("models", &ArgSchema::none(S::View)),
    ("diagnostics", &ArgSchema::none(S::View)),
    ("new", &ArgSchema::none(S::Project)),
    ("new-linked", &ArgSchema::none(S::Project)),
    ("original-linked", &ArgSchema::none(S::Project)),
    ("youtube", &ArgSchema::none(S::Project)),
    ("new-url", &ArgSchema::none(S::Project)),
    ("open", &ArgSchema::none(S::Project)),
    ("close", &ArgSchema::none(S::Project)),
    ("moment-delete", &ArgSchema::none(S::TimelineCursor)),
];
