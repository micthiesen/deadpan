# Inner Repeat PCM regression (written, not yet executed)

Owner files: stages/clocked_speed_update/inner_repeat_pcm.rs and parent module registration only.

The fixture uses public play/gap override commands, WrapRetime(group15->30), then InsertTime(root3,3), giving exact owner33/input33/2. Outside and group lead/tail are silent Holds to admit the audio-only WAV fixture. Repeat bodies retain Source2+RoomTone1, owned middle Source3+RoomTone1, 3 stable plays, owned first RoomTone gap2 and default middle RoomTone gap1.

Known PCM is pcm-mono-44100.wav, with natural-rate Source input3100..23100 and RoomTone input3100..3260. The span is deliberately long enough for each Source's current2/3-frame support. The eight old physical carriers are read individually via bound.raw_domain + StageAudio.read_domain, never by sampling the old repeated bus. This reuses unchanged native Source/RoomTone preparation but independently specifies all retained transfer math and output placement.

Literal old absolute sample supports:6406..12813,12813..16016,16016..22422,22422..32032,32032..35235,35235..38438,38438..44845,44845..48048. Their current paused allocations are11211..17618,17618..20821,20821..27227,27227..36837,36837..40040,40040..43243,43243..49650,49650..52853.

After33->18, mapping current rootx to2+(x-2)*6/11 yields literal allocations7571..11066,11066..12813,12813..16307,16307..21549,21549..23296,23296..25043,25043..28538,28538..30285. Each physical transfer anchors to its old sample-support start and uses step11/6. Each transfer carries the raw physical suppression mask. No native slope is composed with11/6 and no repeated bus is transferred.

The test checks exact individual Bound owner/iteration/gap, complete physical raw support, allocation, anchor and rate before/after; compares actual StageAudio PCM and masks over all eight contiguous carriers including every seam in two alternate chunk patterns; checks inner bindings/overrides/repeat identity; public inverse and JSON through every helper edit; excludes a whole-Repeat binding; then performs public18->33 and compares to the same independently assembled original per-carrier oracle.

No Cargo, Git, or runtime claims by this worker. Root owns execution and evidence.
