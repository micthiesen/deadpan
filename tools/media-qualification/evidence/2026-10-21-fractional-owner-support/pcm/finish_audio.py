from pathlib import Path
p = Path('/Users/michael/Code/deadpan/crates/deadpan-audio/tests/stages/processing_hold_duration_output_sound.rs')
s = p.read_text()
a = s.index('    // Existing limitation, independently present before the target edit:')
b = s.index('    let processor = ntsc_boundary(4)..ntsc_boundary(8);', a)
s = s[:a] + '''    // The structural probe chooses the following Hold. Its exact local clock
    // stays signed; query results are checked against literals and never supply
    // treatment values or processing order to the independent scalar oracle.
    for (document, raw) in [(&before, &physical[0]), (&after, &physical[1])] {
        let plan = RenderPlan::compile(document).unwrap();
        for (at, local) in [(8236, ratio(-1, 4576)), (8237, ratio(1, 1144))] {
            let owners = plan.audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default()).unwrap();
            assert_eq!(owners.spans().len(), 1);
            let span = &owners.spans()[0];
            assert_eq!(span.support(), deadpan_plan::AudioOwnerSupport::Active);
            assert_eq!(span.owners().iter().map(|owner| owner.instance().node.clone()).collect::<Vec<_>>(), vec![id("root"), id("processor"), id("z-body"), id("pause")]);
            let hold = span.owners().last().unwrap();
            assert_eq!(hold.sampling().local_at(AudioSample(at)).unwrap(), local);
            assert_eq!(hold.sampling().local_frames_per_sample(), ratio(35, 32_032));
        }
        let exhausted = plan.audio_gain_owners(AudioSample(12812)..AudioSample(12813), Default::default()).unwrap();
        assert_eq!(exhausted.spans().len(), 1);
        assert_eq!(exhausted.spans()[0].support(), deadpan_plan::AudioOwnerSupport::Inactive);
        assert_eq!(exhausted.spans()[0].owners().iter().map(|owner| owner.instance().node.clone()).collect::<Vec<_>>(), vec![id("root")]);
        assert_eq!(raw[12812], [0.0; 2]);
    }
''' + s[b:]
s = s.replace('fn nonlinear_shared_bus_witness(complete: bool, child_gain: bool)', 'fn nonlinear_shared_bus_witness(include_before: bool, child_gain: bool)')
s = s.replace('let original_gain = |at: usize| -> f64 {', 'let original_gain = |at: usize, hold_end: usize| -> f64 {')
s = s.replace('(8237..10_982).contains(&at)', '(8237..hold_end).contains(&at)')
s = s.replace('let shape = |component: &[[f32; 2]], shared: bool| {', 'let shape = |component: &[[f32; 2]], shared: bool, hold_end: usize| {')
s = s.replace('original_gain(at)', 'original_gain(at, hold_end)')
s = s.replace('shape(&physical[1], true)', 'shape(&physical[1], true, 10_982)')
s = s.replace('shape(&physical[0], true)', 'shape(&physical[0], true, 10_982)')
s = s.replace('shape(&physical[1], false)', 'shape(&physical[1], false, 10_982)')
s = s.replace('* original_gain(at, hold_end),', '* original_gain(at, 10_982),')
a = s.index('    let plan = Arc::new(RenderPlan::compile(&after).unwrap());\n    let mask = physical_suppression(true);', s.index('fn nonlinear_shared_bus_witness'))
b = s.index('\n}\n\n#[test]\nfn fixed_retime_output_sound_proof', a)
s = s[:a] + '''    let previous_expected = shape(&physical[0], true, 10_067);
    let states = if include_before {
        vec![(&before, &previous_expected), (&after, &expected)]
    } else {
        vec![(&after, &expected)]
    };
    let mask = physical_suppression(true);
    for (document, expected) in states {
        let plan = Arc::new(RenderPlan::compile(document).unwrap());
        // Every sample is covered, including the former gap8236. The distinct
        // exhausted output point12812 remains zero with its exact mask.
        for pieces in [&[256_u32][..], &[127, 3, 251][..]] {
            let mut renderer = StageAudio::new(Arc::clone(&plan));
            let mut at = 0;
            for count in pieces.iter().copied().cycle() {
                if at == 17_618 { break; }
                let count = count.min((17_618 - at) as u32);
                let actual = bus(&mut renderer, &mut provider, at, count);
                same_pcm(&actual.samples, &expected[at as usize..at as usize + count as usize], at, "complete shared processor then root saturation");
                assert_eq!(actual.suppressed, mask[at as usize..at as usize + count as usize]);
                at += i64::from(count);
            }
        }
        for (at, count) in [
            (12_700, 114), (6442, 111), (9600, 256), (6406, 37),
            (8192, 44), (8237, 128), (8236, 1), (8192, 128), (8235, 3),
            (10_066, 3), (10_981, 3),
        ] {
            let mut cold = StageAudio::new(Arc::clone(&plan));
            let actual = bus(&mut cold, &mut provider, at, count);
            assert_eq!(actual.samples, expected[at as usize..at as usize + count as usize]);
            assert_eq!(actual.suppressed, mask[at as usize..at as usize + count as usize]);
        }
    }
''' + s[b:]
assert 'if !complete' not in s
assert 'if complete' not in s
p.write_text(s)
