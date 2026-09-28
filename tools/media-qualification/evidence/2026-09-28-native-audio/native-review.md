# Independent review and execution boundaries

The native observer and original adapter had independent read-only review before
integration. The public timing-only PcmSpan/event extraction, final native
adapter, recorder and runner received a second bounded review. The sole runner
finding was loss of the selected sanitizer load-path spelling after resolving
its target. The fix retains both candidate resolution and bytes, with a test
that retargets a symlink to different but byte-identical content. Re-review
confirmed the correction.

The first real read exposed a producer/schema issue: CoreMedia's unsigned-byte
Boolean boxed as numeric JSON 1. The oracle rejected it without relaxing its
boolean admission. Three producer boxing sites now cast to C bool explicitly.
The original source and failed observation remain retained. Re-review confirmed
the narrow fix. Both actual corrected reads and sanitizer reads then completed;
the no-edit-list case still failed timing, as reported.

The parent alone ran the 93 tests, reused-observation regression, compiler,
native readers and ASan/UBSan. No concurrent compiler/media/GPU invocation ran.
Independent Rust renderer authoring continued, but the seven native source
dependencies remained frozen through each run. No Rust or GUI gate is claimed
for those concurrent, still-unverified changes.
