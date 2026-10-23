from pathlib import Path
p=Path('/Users/michael/Code/deadpan/crates/deadpan-core/tests/compact_audio_bindings.rs')
text=p.read_text()
start=text.index('    let document = document_with_hold();',text.index('fn audio_binding_state_preflight_accepts_'))
end=text.index('    let mut explicit_value:',start)
construction=text[start:end]
assert construction.endswith('    state.validate_for(&document).unwrap();\n')
helper='fn nested_state() -> AudioBindingState {\n'+construction+'    state\n}\n\n'
anchor='#[test]\nfn audio_binding_state_preflight_accepts_'
assert text.count(anchor)==1
text=text[:start]+'    let state = nested_state();\n'+text[end:]
text=text.replace(anchor,helper+anchor)
start=text.index('    let document = document_with_hold();',text.index('fn audio_binding_state_preflight_rejects_'))
end=text.index('    let base: Value',start)
text=text[:start]+'    let state = nested_state();\n'+text[end:]
start=text.index('    let mut null_path = base.clone();')
end=text.index('    // Exercise the raw-JSON preflight',start)
text=text[:start]+'''    assert_eq!(AudioBindingState::from_json(&base.to_string()).unwrap(), state);
    let placements = [
        "/bindings/owner/lattice",
        "/bindings/owner/resume/phase/terms/0/placement",
        "/bindings/owner/reanchors/0/placement",
    ];
    for placement in placements {
        for path in ["arguments", "births"] {
            let mut null = base.clone();
            null.pointer_mut(placement).unwrap()[path] = Value::Null;
            assert!(AudioBindingState::from_json(&null.to_string()).is_err(),
                "accepted null {placement}/{path}");
            // Deliberately opaque entries prove the bounded raw pass rejects
            // the collection before interpreting its members.
            let mut oversized = base.clone();
            oversized.pointer_mut(placement).unwrap()[path] =
                json!(vec![Value::Null; MAX_DOCUMENT_DEPTH + 1]);
            let error = AudioBindingState::from_json(&oversized.to_string()).unwrap_err();
            assert!(error.message.contains("audio binding sequence limit"),
                "wrong refusal for {placement}/{path}: {error:?}");
        }
    }
    let compact = state.to_json().unwrap();
''' +text[end:]
out=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23/refined-compact-audio-bindings.rs.txt')
out.write_text(text)
print('Prepared fixture correction; production unchanged',len(text))
