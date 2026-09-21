# Video admission evidence

`gate/` records the final repository gate and 486 passing Rust tests.
`sanitizer/` records 93 passing source/media tests with C-adapter ASan/UBSan;
Rust and FFmpeg libraries are not instrumented.
`source-probes/` records six actual historical-source reopens and random seeks.
`ffv1/` retains the independent native configuration comparison.
`matroska-tags/` retains small tag-expansion reproductions and probe sources.
`iterations/` retains initial failures and the tag regression before/after fix.

Scratch harnesses are captured research tools, not application components or
end-user dependencies. Some retain the original absolute developer paths.
Do not run the deeper tag-expansion cases without an external time limit; the
production admission guard rejects them before native parsing.
The detailed scope and incomplete review check are in the linked
[qualification record](../../../../docs/qualification/video-admission-2026-09-21.md).
