# Native gain review

2026-09-28, macOS 26.5.2 / Apple M5 Max. The parent used CUA against the
private `Deadpan Gain Review.app` bundle and the retained corrected-gain fixture.
`build.json` binds the copied bundle to Cargo's exact verified base-app executable;
its SHA-256 is `1c814907486ff04f46f3a82704ace2200c60e10e7ad3beed5130de8eedb4d969`.
The compiled source manifest is
`533439a4ece260aa1ee56751987ff765d8cd3601c52eafa0f323743a29b41dbe`.

Observed through the actual native window, keyboard events and accessibility
state, without direct project-model mutations:

1. `:` followed by `gain` and Return opened the unsaved editor and focused
   `Gain draft keyboard focus`.
2. Shift+Tab reached `Cancel · Esc`. Two forward Tabs wrapped through the
   heading and reached `Whole beat trim · dB` without an empty stop.
3. Cmd+A, `-3.125`, Tab, Return activated `Set trim`. The field and inspector
   showed exactly -3.125 dB while the editor remained visibly unsaved.
4. Four Tabs and Return reached and activated `Add envelope`. The inspector
   showed one envelope; the graph and exact fields appeared.
5. Eleven Tabs reached `Value · dB`. The native scroller revealed its complete
   label/input, together with the owner-clock graph. The retained picture
   remained visible above the editor.
6. Cmd+A, `1.5`, Tab, Return activated `Update key` inside the draft. No project
   Apply action occurred.
7. Shift+Tab, Cmd+A and `dd + y` left those shortcut characters inside the value
   field. The UI showed Unapplied fields and disabled Apply/new audition.
8. Escape restored Normal mode, Viewer context, boundary 0/120, 0 dB and zero
   envelopes/mute ranges. Undo remained disabled.
9. Quit closed the fixture. A post-quit CUA observation reopened an empty app;
   this extra instance was quit without another observation. A bounded process
   query confirmed no private review instance remained.

The full headless project dump after closing is byte-identical to the dump
before opening: SHA-256
`b128bf0fe5d6ec9143681ad7e04c9429cabb1ac4cab41d64c4d6033856007ff9`.
`comparison.json` and compressed before/after dumps retain that check.

Native screenshots were inspected in the tool transcript; the repository's
selected PNGs are separately identified actual Metal replay captures. CUA
keyboard injection is not a physical keyboard-layout or OS IME certification.
No device playback, listening, VoiceOver or physical display timing was tested
in this native pass. Real PCM and replay delivery have their own evidence.
