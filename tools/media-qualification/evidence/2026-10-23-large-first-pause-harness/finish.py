from pathlib import Path
p=Path('/tmp/deadpan-large-pause-harness-2026-10-23/draft.py')
s=p.read_text()
s=s.replace('manifest={}', '''
modified[h] = modified[h].replace(
    'let copy = root.join("generated-playback.deadpan");',
    'let copy = root.join(if name == "large-first-pause" { "large-first-pause.deadpan" } else { "generated-playback.deadpan" });'
)
modified[new] = modified[new].replace(
    '                && background_hold(document.nodes().get(id), 12)',
    \'''                && document.nodes().get(id) == Some(&deadpan_core::BeatNode::hold(
                    format!("Scale fixture {:06}", index + 1),
                    deadpan_core::HoldRecipe {
                        picture_context: None,
                        duration: deadpan_core::FrameDuration::new(12).expect("constant positive duration"),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                    },
                ))\'''
)
manifest={}''')
p.write_text(s)
