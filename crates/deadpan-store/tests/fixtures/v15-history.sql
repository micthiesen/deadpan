-- Genuine schema-15 project generated and validated with commit
-- d1348a5dbf2a4979a54af823b4a4f5ca82ed44e1 (core schema 10).
-- Starts from v14-history.sql, migrated by that revision's rebuilt CLI.
-- The old host API imports a primary source, adopts measured geometry, then
-- edits the canvas and leaves a pending redo. Earlier branches, qualifications,
-- original ownership and operational generation rows remain in the fixture.
-- Generated twice by tools/media-qualification/evidence/
-- 2026-09-23-audio-edges/fixture/generate.py, byte-for-byte equal.
-- SQLite backup captures the metadata; managed media bytes are not embedded.
PRAGMA application_id=1146113585;
PRAGMA user_version=15;
BEGIN TRANSACTION;
CREATE TABLE generation_attempt_heads (
    request_id TEXT PRIMARY KEY REFERENCES generation_requests(request_id),
    high_water INTEGER NOT NULL CHECK (high_water BETWEEN 1 AND 9223372036854775807),
    latest_attempt_id TEXT NOT NULL,
    selected_ready_attempt_id TEXT,
    FOREIGN KEY (request_id,latest_attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id),
    FOREIGN KEY (request_id,selected_ready_attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;
INSERT INTO "generation_attempt_heads" VALUES('probe-ad82362664564e1a97c381111aaad5aa',1,'attempt-1','attempt-1');
CREATE TABLE generation_attempts (
    request_id TEXT NOT NULL REFERENCES generation_requests(request_id),
    attempt_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 9223372036854775807),
    cancellation_token TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'queued','preflight','loading','running','validating','ready','failed','cancelling','cancelled'
    )),
    worker_stage TEXT CHECK (worker_stage IS NULL OR worker_stage IN (
        'preflight','runtime_loading','model_loading','conditioning','inference',
        'decoding','encoding','worker_validation'
    )),
    transition_sequence INTEGER NOT NULL
        CHECK (transition_sequence BETWEEN 1 AND 9223372036854775807),
    cancel_response TEXT CHECK (cancel_response IS NULL OR cancel_response IN ('cancelled','completed')),
    worker_candidate TEXT CHECK (worker_candidate IS NULL OR json_valid(worker_candidate)),
    failure_origin TEXT CHECK (failure_origin IS NULL OR failure_origin IN ('worker','host')),
    failure_code TEXT,
    failure_detail TEXT,
    PRIMARY KEY (request_id,attempt_id),
    UNIQUE (request_id,ordinal),
    CHECK ((failure_origin IS NULL) = (failure_code IS NULL)),
    CHECK ((failure_origin IS NULL) = (failure_detail IS NULL)),
    CHECK (cancel_response!='completed' OR worker_candidate IS NOT NULL)
) STRICT;
INSERT INTO "generation_attempts" VALUES('probe-ad82362664564e1a97c381111aaad5aa','attempt-1',1,'acceptance-probe-token','ready','inference',5,NULL,'{"native":{"reference":"outputs/native.mp4","sha256":"c9d34268df14d105bb4f3799e9bfc9b946ad153de43e6ee4a73a44bb7833be33","byte_length":4911319},"provenance":{"reference":"outputs/provenance.json","sha256":"7a74f15d64d30f98035b43193a4c624cb0102325b54e73eccf7405c5e7d5e18e","byte_length":37241},"video":{"frames":25,"frame_rate":{"numerator":24,"denominator":1},"width":768,"height":320},"provider":{"pack_id":"ltx-2.3-q4-development","pack_version":"56a5866d","runtime_id":"ltx-mlx-development","runtime_version":"0.15.8+deadpan1","seed":1}}',NULL,NULL,NULL);
CREATE TABLE generation_bundle_receipts (
    request_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    bundle TEXT NOT NULL CHECK (json_valid(bundle)),
    availability TEXT NOT NULL CHECK (availability IN ('present','evicted')),
    PRIMARY KEY (request_id,attempt_id),
    FOREIGN KEY (request_id,attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;
INSERT INTO "generation_bundle_receipts" VALUES('probe-ad82362664564e1a97c381111aaad5aa','attempt-1','{"native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"provenance_object":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"native_video":{"frames":25,"frame_rate":{"numerator":24,"denominator":1},"width":768,"height":320},"sampled_video":{"frames":30,"frame_rate":{"numerator":30000,"denominator":1001},"width":768,"height":320},"plan":{"schema_version":1,"operation":"bridge","interpolation":"linear","project":{"interior_frames":30,"frame_rate":{"numerator":30000,"denominator":1001}},"native":{"frame_count":25,"frame_rate":{"numerator":24,"denominator":1},"width":768,"height":320},"timing":{"requested_boundary_duration":{"numerator":"31031","denominator":"30000"},"actual_boundary_duration":{"numerator":"1","denominator":"1"},"retime_deviation":{"numerator":"-1031","denominator":"30000"}},"sampling":{"endpoint_policy":"interior_only"}},"provider":{"pack_id":"ltx-2.3-q4-development","pack_version":"56a5866d","runtime_id":"ltx-mlx-development","runtime_version":"0.15.8+deadpan1","seed":1},"native_sha256":"c9d34268df14d105bb4f3799e9bfc9b946ad153de43e6ee4a73a44bb7833be33","native_byte_length":4911319,"provenance_sha256":"7a74f15d64d30f98035b43193a4c624cb0102325b54e73eccf7405c5e7d5e18e","provenance_byte_length":37241,"validator":{"id":"native-ffv1","version":"bridge-3"},"availability":"present","admission":{"native_span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"sampled_span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"inputs":{"context_sha256":"54e84510a24b18437e79145c9cca90230bf0e38d48d72107006ac3f2e0de5eb4","manifest":{"content":{"algorithm":"blake3","digest":"fa905adba9f3b7b84cb1ade4b537ec3a2c7514c94bdcc3c4c16735f8d112fa92"},"byte_length":1350},"left":{"content":{"algorithm":"blake3","digest":"7074ad08d6a13fddcc615c7bf2fc4d7d3cd67320ae813e0f4af83e3c544ccf3e"},"byte_length":310963},"right":{"content":{"algorithm":"blake3","digest":"1546d7b105d6fcf262b93fabc5683642358930f897e0bd1c28617c1141482c3a"},"byte_length":310279}}}}','present');
CREATE TABLE generation_candidate_receipts (
    request_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    staged_ref TEXT NOT NULL UNIQUE,
    sha256 TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length BETWEEN 1 AND 9223372036854775807),
    video TEXT NOT NULL CHECK (json_valid(video)),
    provider TEXT NOT NULL CHECK (json_valid(provider)),
    validator_id TEXT NOT NULL,
    validator_version TEXT NOT NULL,
    availability TEXT NOT NULL CHECK (availability IN ('present','evicted')),
    PRIMARY KEY (request_id,attempt_id),
    FOREIGN KEY (request_id,attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;
CREATE TABLE generation_requests (
    request_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    hold_id TEXT NOT NULL,
    request_version INTEGER NOT NULL
        CHECK (request_version BETWEEN 1 AND 9223372036854775807),
    origin_revision TEXT NOT NULL REFERENCES revisions(id),
    context_sha256 TEXT NOT NULL,
    constraints TEXT NOT NULL CHECK (json_valid(constraints)),
    provider TEXT NOT NULL CHECK (json_valid(provider)),
    bridge_plan TEXT CHECK (bridge_plan IS NULL OR json_valid(bridge_plan)),
    relevance TEXT NOT NULL CHECK (relevance IN ('current','stale','detached')),
    UNIQUE (hold_id, request_version)
) STRICT;
INSERT INTO "generation_requests" VALUES('probe-ad82362664564e1a97c381111aaad5aa','probe-ad82362664564e1a97c381111aaad5aa','hold-1',1,'revision-1','54e84510a24b18437e79145c9cca90230bf0e38d48d72107006ac3f2e0de5eb4','{"video":{"frames":30,"frame_rate":{"numerator":30000,"denominator":1001},"width":768,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"ltx-2.3-q4-development","pack_version":"56a5866d","runtime_id":"ltx-mlx-development","runtime_version":"0.15.8+deadpan1","seed":1}','{"schema_version":1,"operation":"bridge","interpolation":"linear","project":{"interior_frames":30,"frame_rate":{"numerator":30000,"denominator":1001}},"native":{"frame_count":25,"frame_rate":{"numerator":24,"denominator":1},"width":768,"height":320},"timing":{"requested_boundary_duration":{"numerator":"31031","denominator":"30000"},"actual_boundary_duration":{"numerator":"1","denominator":"1"},"retime_deviation":{"numerator":"-1031","denominator":"30000"}},"sampling":{"endpoint_policy":"interior_only"}}','current');
CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
INSERT INTO "history" VALUES(1,NULL,'accepted','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"revision-1","new_revision":"accepted","command":{"command":"accept_generated_hold","node":"hold-1","artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"assets":{"accepted-native":{"label":"Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","content_hash":"blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":25},"accepted-sampled":{"label":"Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","content_hash":"blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":30}}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"revision-1","to_revision":"accepted","nodes":{"hold-1":{"before":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{"accepted-native":{"before":null,"after":{"label":"Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","content_hash":"blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":25}},"accepted-sampled":{"before":null,"after":{"label":"Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","content_hash":"blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":30}}},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"accepted","to_revision":"revision-1","nodes":{"hold-1":{"before":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{"accepted-native":{"before":{"label":"Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","content_hash":"blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":25},"after":null},"accepted-sampled":{"before":{"label":"Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","content_hash":"blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":30},"after":null}},"marks":{},"overrides":{}},"changed_ids":["hold-1"],"duration_delta":0,"description":"Accept generated hold"}');
INSERT INTO "history" VALUES(2,1,'revert-to-fallback','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"redo-accept","new_revision":"revert-to-fallback","command":{"command":"revert_generated_hold","node":"hold-1"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"redo-accept","to_revision":"revert-to-fallback","nodes":{"hold-1":{"before":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"revert-to-fallback","to_revision":"redo-accept","nodes":{"hold-1":{"before":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["hold-1"],"duration_delta":0,"description":"Revert generated hold"}');
INSERT INTO "history" VALUES(3,2,'schema10-add-av-asset','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"revert-to-fallback","new_revision":"schema10-add-av-asset","command":{"command":"add_asset","id":"original-av","asset":{"label":"Original A/V with distinct origins","content_hash":"7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}},"audio":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"revert-to-fallback","to_revision":"schema10-add-av-asset","nodes":{},"assets":{"original-av":{"before":null,"after":{"label":"Original A/V with distinct origins","content_hash":"7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}},"audio":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120}}},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-add-av-asset","to_revision":"revert-to-fallback","nodes":{},"assets":{"original-av":{"before":{"label":"Original A/V with distinct origins","content_hash":"7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}},"audio":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120},"after":null}},"marks":{},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Register media asset"}');
INSERT INTO "history" VALUES(4,3,'schema10-insert-negative','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema10-add-av-asset","new_revision":"schema10-insert-negative","command":{"command":"insert","parent":"acceptance-probe-root","index":1,"subtree":{"root":"source-negative","nodes":{"source-negative":{"label":"source-negative","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":-137}}}},"overrides":{}}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-add-av-asset","to_revision":"schema10-insert-negative","nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative"]}}},"source-negative":{"before":null,"after":{"label":"source-negative","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-insert-negative","to_revision":"schema10-add-av-asset","nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1"]}}},"source-negative":{"before":{"label":"source-negative","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":-137}}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["acceptance-probe-root","source-negative"],"duration_delta":60,"description":"Insert beats"}');
INSERT INTO "history" VALUES(5,4,'schema10-insert-positive','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema10-insert-negative","new_revision":"schema10-insert-positive","command":{"command":"insert","parent":"acceptance-probe-root","index":2,"subtree":{"root":"source-positive","nodes":{"source-positive":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":2401}}}},"overrides":{}}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-insert-negative","to_revision":"schema10-insert-positive","nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative","source-positive"]}}},"source-positive":{"before":null,"after":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":2401}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-insert-positive","to_revision":"schema10-insert-negative","nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative","source-positive"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative"]}}},"source-positive":{"before":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":2401}}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["acceptance-probe-root","source-positive"],"duration_delta":60,"description":"Insert beats"}');
INSERT INTO "history" VALUES(6,5,'schema10-source-mark','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema10-insert-positive","new_revision":"schema10-source-mark","command":{"command":"set_mark","id":"source-audio-mark","owner":"source-negative","label":"Original audio zero","boundary":{"coordinate":{"space":"source","asset":"original-av","moment":{"type":"audio_sample","sample":0,"sample_rate":48000}},"bias":"right"},"loss_policy":"keep_unresolved"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-insert-positive","to_revision":"schema10-source-mark","nodes":{},"assets":{},"marks":{"source-audio-mark":{"before":null,"after":{"owner":"source-negative","label":"Original audio zero","boundary":{"coordinate":{"space":"source","asset":"original-av","moment":{"type":"audio_sample","sample":0,"sample_rate":48000}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}}}},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-source-mark","to_revision":"schema10-insert-positive","nodes":{},"assets":{},"marks":{"source-audio-mark":{"before":{"owner":"source-negative","label":"Original audio zero","boundary":{"coordinate":{"space":"source","asset":"original-av","moment":{"type":"audio_sample","sample":0,"sample_rate":48000}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}},"after":null}},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Set mark"}');
INSERT INTO "history" VALUES(7,6,'schema10-local-mark','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema10-source-mark","new_revision":"schema10-local-mark","command":{"command":"set_mark","id":"local-mark","owner":"source-positive","label":"Exact local boundary","boundary":{"coordinate":{"space":"local","node":"source-positive","position":{"numerator":"7","denominator":"3"}},"bias":"left"},"loss_policy":"keep_unresolved"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-source-mark","to_revision":"schema10-local-mark","nodes":{},"assets":{},"marks":{"local-mark":{"before":null,"after":{"owner":"source-positive","label":"Exact local boundary","boundary":{"coordinate":{"space":"local","node":"source-positive","position":{"numerator":"7","denominator":"3"}},"bias":"left"},"loss_policy":"keep_unresolved","state":{"type":"bound"}}}},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-local-mark","to_revision":"schema10-source-mark","nodes":{},"assets":{},"marks":{"local-mark":{"before":{"owner":"source-positive","label":"Exact local boundary","boundary":{"coordinate":{"space":"local","node":"source-positive","position":{"numerator":"7","denominator":"3"}},"bias":"left"},"loss_policy":"keep_unresolved","state":{"type":"bound"}},"after":null}},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Set mark"}');
INSERT INTO "history" VALUES(8,7,'schema10-rename-source','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema10-local-mark","new_revision":"schema10-rename-source","command":{"command":"rename","node":"source-negative","label":"Renamed source with negative offset"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-local-mark","to_revision":"schema10-rename-source","nodes":{"source-negative":{"before":{"label":"source-negative","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":-137}}},"after":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema10-rename-source","to_revision":"schema10-local-mark","nodes":{"source-negative":{"before":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":-137}}},"after":{"label":"source-negative","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(9,8,'schema11-audio-negative','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema11-redo-inherited","new_revision":"schema11-audio-negative","command":{"command":"set_source_audio_mapping","node":"source-negative","mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"offset":-137}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-redo-inherited","to_revision":"schema11-audio-negative","nodes":{"source-negative":{"before":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":-137}}},"after":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-audio-negative","to_revision":"schema11-redo-inherited","nodes":{"source-negative":{"before":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Change source audio mapping"}');
INSERT INTO "history" VALUES(10,9,'schema11-audio-positive','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema11-audio-negative","new_revision":"schema11-audio-positive","command":{"command":"edit_occurrence","instance":{"node":"source-positive","repeats":[]},"edit":{"type":"set_source_audio_mapping","mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"offset":2401},"identities":{"nodes":[],"marks":[]}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-audio-negative","to_revision":"schema11-audio-positive","nodes":{"source-positive":{"before":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":2401}}},"after":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-audio-positive","to_revision":"schema11-audio-negative","nodes":{"source-positive":{"before":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}},"after":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"fit_beat"},"link":"linked","audio_offset":2401}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-positive"],"duration_delta":0,"description":"Edit selected occurrence"}');
INSERT INTO "history" VALUES(11,10,'schema11-local-mark','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema11-audio-positive","new_revision":"schema11-local-mark","command":{"command":"set_mark","id":"mapping-local-mark","owner":"source-negative","label":"Exact mapping boundary","boundary":{"coordinate":{"space":"local","node":"source-negative","position":{"numerator":"13","denominator":"7"}},"bias":"right"},"loss_policy":"keep_unresolved"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-audio-positive","to_revision":"schema11-local-mark","nodes":{},"assets":{},"marks":{"mapping-local-mark":{"before":null,"after":{"owner":"source-negative","label":"Exact mapping boundary","boundary":{"coordinate":{"space":"local","node":"source-negative","position":{"numerator":"13","denominator":"7"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}}}},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-local-mark","to_revision":"schema11-audio-positive","nodes":{},"assets":{},"marks":{"mapping-local-mark":{"before":{"owner":"source-negative","label":"Exact mapping boundary","boundary":{"coordinate":{"space":"local","node":"source-negative","position":{"numerator":"13","denominator":"7"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}},"after":null}},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Set mark"}');
INSERT INTO "history" VALUES(12,11,'schema11-abandoned-rename','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema11-local-mark","new_revision":"schema11-abandoned-rename","command":{"command":"rename","node":"source-negative","label":"Abandoned schema 11 branch"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-local-mark","to_revision":"schema11-abandoned-rename","nodes":{"source-negative":{"before":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Abandoned schema 11 branch","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-abandoned-rename","to_revision":"schema11-local-mark","nodes":{"source-negative":{"before":{"label":"Abandoned schema 11 branch","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(13,11,'schema11-rename-branch','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema11-undo-abandoned","new_revision":"schema11-rename-branch","command":{"command":"rename","node":"source-negative","label":"Schema 11 independent audio"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-undo-abandoned","to_revision":"schema11-rename-branch","nodes":{"source-negative":{"before":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema11-rename-branch","to_revision":"schema11-undo-abandoned","nodes":{"source-negative":{"before":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Renamed source with negative offset","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(14,13,'schema12-video-negative','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema12-redo-inherited","new_revision":"schema12-video-negative","command":{"command":"set_source_video_mapping","node":"source-negative","mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-redo-inherited","to_revision":"schema12-video-negative","nodes":{"source-negative":{"before":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-video-negative","to_revision":"schema12-redo-inherited","nodes":{"source-negative":{"before":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Change source video mapping"}');
INSERT INTO "history" VALUES(15,14,'schema12-video-positive','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema12-video-negative","new_revision":"schema12-video-positive","command":{"command":"edit_occurrence","instance":{"node":"source-positive","repeats":[]},"edit":{"type":"set_source_video_mapping","mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"}},"identities":{"nodes":[],"marks":[]}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-video-negative","to_revision":"schema12-video-positive","nodes":{"source-positive":{"before":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}},"after":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-video-positive","to_revision":"schema12-video-negative","nodes":{"source-positive":{"before":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}},"after":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-positive"],"duration_delta":0,"description":"Edit selected occurrence"}');
INSERT INTO "history" VALUES(16,15,'schema12-local-mark','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema12-video-positive","new_revision":"schema12-local-mark","command":{"command":"set_mark","id":"picture-mapping-local-mark","owner":"source-negative","label":"Exact picture mapping boundary","boundary":{"coordinate":{"space":"local","node":"source-negative","position":{"numerator":"13","denominator":"7"}},"bias":"right"},"loss_policy":"keep_unresolved"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-video-positive","to_revision":"schema12-local-mark","nodes":{},"assets":{},"marks":{"picture-mapping-local-mark":{"before":null,"after":{"owner":"source-negative","label":"Exact picture mapping boundary","boundary":{"coordinate":{"space":"local","node":"source-negative","position":{"numerator":"13","denominator":"7"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}}}},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-local-mark","to_revision":"schema12-video-positive","nodes":{},"assets":{},"marks":{"picture-mapping-local-mark":{"before":{"owner":"source-negative","label":"Exact picture mapping boundary","boundary":{"coordinate":{"space":"local","node":"source-negative","position":{"numerator":"13","denominator":"7"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}},"after":null}},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Set mark"}');
INSERT INTO "history" VALUES(17,16,'schema12-abandoned-rename','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema12-local-mark","new_revision":"schema12-abandoned-rename","command":{"command":"rename","node":"source-negative","label":"Abandoned schema 12 branch"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-local-mark","to_revision":"schema12-abandoned-rename","nodes":{"source-negative":{"before":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Abandoned schema 12 branch","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-abandoned-rename","to_revision":"schema12-local-mark","nodes":{"source-negative":{"before":{"label":"Abandoned schema 12 branch","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(18,16,'schema12-rename-branch','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema12-undo-abandoned","new_revision":"schema12-rename-branch","command":{"command":"rename","node":"source-negative","label":"Schema 12 independent streams"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-undo-abandoned","to_revision":"schema12-rename-branch","nodes":{"source-negative":{"before":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema12-rename-branch","to_revision":"schema12-undo-abandoned","nodes":{"source-negative":{"before":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 11 independent audio","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(19,18,'schema13-place-video-negative','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema13-redo-inherited","new_revision":"schema13-place-video-negative","command":{"command":"set_source_video_mapping","node":"source-negative","mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-redo-inherited","to_revision":"schema13-place-video-negative","nodes":{"source-negative":{"before":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-place-video-negative","to_revision":"schema13-redo-inherited","nodes":{"source-negative":{"before":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Change source video mapping"}');
INSERT INTO "history" VALUES(20,19,'schema13-place-audio-negative','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema13-place-video-negative","new_revision":"schema13-place-audio-negative","command":{"command":"edit_occurrence","instance":{"node":"source-negative","repeats":[]},"edit":{"type":"set_source_audio_mapping","mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"offset":-137},"identities":{"nodes":[],"marks":[]}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-place-video-negative","to_revision":"schema13-place-audio-negative","nodes":{"source-negative":{"before":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-place-audio-negative","to_revision":"schema13-place-video-negative","nodes":{"source-negative":{"before":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Edit selected occurrence"}');
INSERT INTO "history" VALUES(21,20,'schema13-place-video-positive','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema13-place-audio-negative","new_revision":"schema13-place-video-positive","command":{"command":"edit_occurrence","instance":{"node":"source-positive","repeats":[]},"edit":{"type":"set_source_video_mapping","mapping":{"type":"placement","start":{"numerator":"-3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"}},"identities":{"nodes":[],"marks":[]}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-place-audio-negative","to_revision":"schema13-place-video-positive","nodes":{"source-positive":{"before":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}},"after":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"-3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-place-video-positive","to_revision":"schema13-place-audio-negative","nodes":{"source-positive":{"before":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"-3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}},"after":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-positive"],"duration_delta":0,"description":"Edit selected occurrence"}');
INSERT INTO "history" VALUES(22,21,'schema13-place-audio-positive','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema13-place-video-positive","new_revision":"schema13-place-audio-positive","command":{"command":"set_source_audio_mapping","node":"source-positive","mapping":{"type":"placement","start":{"numerator":"3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"}},"offset":2401}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-place-video-positive","to_revision":"schema13-place-audio-positive","nodes":{"source-positive":{"before":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"-3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}},"after":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"-3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-place-audio-positive","to_revision":"schema13-place-video-positive","nodes":{"source-positive":{"before":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"-3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}},"after":{"label":"source-positive","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"-3","denominator":"7"},"frames":{"numerator":"120000","denominator":"1001"},"endpoints":"reject"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"duration","frames":{"numerator":"120000","denominator":"1001"}},"link":"linked","audio_offset":2401}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-positive"],"duration_delta":0,"description":"Change source audio mapping"}');
INSERT INTO "history" VALUES(23,22,'schema13-local-mark','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema13-place-audio-positive","new_revision":"schema13-local-mark","command":{"command":"set_mark","id":"placement-local-mark","owner":"source-negative","label":"Exact signed placement boundary","boundary":{"coordinate":{"space":"local","node":"source-negative","position":{"numerator":"13","denominator":"7"}},"bias":"right"},"loss_policy":"keep_unresolved"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-place-audio-positive","to_revision":"schema13-local-mark","nodes":{},"assets":{},"marks":{"placement-local-mark":{"before":null,"after":{"owner":"source-negative","label":"Exact signed placement boundary","boundary":{"coordinate":{"space":"local","node":"source-negative","position":{"numerator":"13","denominator":"7"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}}}},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-local-mark","to_revision":"schema13-place-audio-positive","nodes":{},"assets":{},"marks":{"placement-local-mark":{"before":{"owner":"source-negative","label":"Exact signed placement boundary","boundary":{"coordinate":{"space":"local","node":"source-negative","position":{"numerator":"13","denominator":"7"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}},"after":null}},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Set mark"}');
INSERT INTO "history" VALUES(24,23,'schema13-abandoned-rename','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema13-local-mark","new_revision":"schema13-abandoned-rename","command":{"command":"rename","node":"source-negative","label":"Abandoned schema 13 branch"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-local-mark","to_revision":"schema13-abandoned-rename","nodes":{"source-negative":{"before":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Abandoned schema 13 branch","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-abandoned-rename","to_revision":"schema13-local-mark","nodes":{"source-negative":{"before":{"label":"Abandoned schema 13 branch","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(25,23,'schema13-rename-branch','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema13-undo-abandoned","new_revision":"schema13-rename-branch","command":{"command":"rename","node":"source-negative","label":"Schema 13 signed stream placements"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-undo-abandoned","to_revision":"schema13-rename-branch","nodes":{"source-negative":{"before":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 13 signed stream placements","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema13-rename-branch","to_revision":"schema13-undo-abandoned","nodes":{"source-negative":{"before":{"label":"Schema 13 signed stream placements","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}},"after":{"label":"Schema 12 independent streams","kind":{"type":"source","source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}}},"video_mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"},"audio":{"asset":"original-av","span":{"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"link":"linked","audio_offset":-137}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source-negative"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(26,25,'schema14-first-import','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema14-redo-inherited","new_revision":"schema14-first-import","command":{"command":"import_source","id":"qualified-camera","asset":{"label":"Qualified cfr-bframes.mp4","content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"},"insertion":{"parent":"acceptance-probe-root","index":0,"node":"first-qualified-clip","label":"Inserted cfr-bframes.mp4","source":{"duration":120,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema14-redo-inherited","to_revision":"schema14-first-import","nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative","source-positive"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["first-qualified-clip","hold-1","source-negative","source-positive"]}}},"first-qualified-clip":{"before":null,"after":{"label":"Inserted cfr-bframes.mp4","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}}}}},"assets":{"qualified-camera":{"before":null,"after":{"label":"Qualified cfr-bframes.mp4","content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema14-first-import","to_revision":"schema14-redo-inherited","nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["first-qualified-clip","hold-1","source-negative","source-positive"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative","source-positive"]}}},"first-qualified-clip":{"before":{"label":"Inserted cfr-bframes.mp4","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}}},"after":null}},"assets":{"qualified-camera":{"before":{"label":"Qualified cfr-bframes.mp4","content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"},"after":null}},"marks":{},"overrides":{}},"changed_ids":["acceptance-probe-root","first-qualified-clip"],"duration_delta":120,"description":"Import source media"}');
INSERT INTO "history" VALUES(27,25,'schema14-second-import','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema14-undo-first-import","new_revision":"schema14-second-import","command":{"command":"import_source","id":"qualified-camera","asset":{"label":"Qualified vfr.mp4","content_hash":"blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"},"insertion":{"parent":"acceptance-probe-root","index":0,"node":"second-qualified-clip","label":"Inserted vfr.mp4","source":{"duration":240,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"238","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"240","denominator":"1"}},"link":"linked","audio_offset":0}}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema14-undo-first-import","to_revision":"schema14-second-import","nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative","source-positive"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["second-qualified-clip","hold-1","source-negative","source-positive"]}}},"second-qualified-clip":{"before":null,"after":{"label":"Inserted vfr.mp4","kind":{"type":"source","source":{"duration":240,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"238","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"240","denominator":"1"}},"link":"linked","audio_offset":0}}}}},"assets":{"qualified-camera":{"before":null,"after":{"label":"Qualified vfr.mp4","content_hash":"blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"}}},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema14-second-import","to_revision":"schema14-undo-first-import","nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["second-qualified-clip","hold-1","source-negative","source-positive"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold-1","source-negative","source-positive"]}}},"second-qualified-clip":{"before":{"label":"Inserted vfr.mp4","kind":{"type":"source","source":{"duration":240,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"238","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"240","denominator":"1"}},"link":"linked","audio_offset":0}}},"after":null}},"assets":{"qualified-camera":{"before":{"label":"Qualified vfr.mp4","content_hash":"blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"},"after":null}},"marks":{},"overrides":{}},"changed_ids":["acceptance-probe-root","second-qualified-clip"],"duration_delta":240,"description":"Import source media"}');
INSERT INTO "history" VALUES(28,27,'schema14-rename','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema14-second-import","new_revision":"schema14-rename","command":{"command":"rename","node":"second-qualified-clip","label":"Schema 14 qualified branch"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema14-second-import","to_revision":"schema14-rename","nodes":{"second-qualified-clip":{"before":{"label":"Inserted vfr.mp4","kind":{"type":"source","source":{"duration":240,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"238","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"240","denominator":"1"}},"link":"linked","audio_offset":0}}},"after":{"label":"Schema 14 qualified branch","kind":{"type":"source","source":{"duration":240,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"238","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"240","denominator":"1"}},"link":"linked","audio_offset":0}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema14-rename","to_revision":"schema14-second-import","nodes":{"second-qualified-clip":{"before":{"label":"Schema 14 qualified branch","kind":{"type":"source","source":{"duration":240,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"238","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"240","denominator":"1"}},"link":"linked","audio_offset":0}}},"after":{"label":"Inserted vfr.mp4","kind":{"type":"source","source":{"duration":240,"video":{"type":"stream","asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":238238,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"238","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"qualified-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":384384,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"240","denominator":"1"}},"link":"linked","audio_offset":0}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["second-qualified-clip"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(29,28,'schema15-primary-import','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema15-redo-inherited","new_revision":"schema15-primary-import","command":{"command":"import_source","id":"schema15-primary-camera","asset":{"label":"Qualified cfr-bframes.mp4","content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"},"insertion":{"parent":"acceptance-probe-root","index":0,"node":"schema15-primary-clip","label":"Inserted cfr-bframes.mp4","source":{"duration":120,"video":{"type":"stream","asset":"schema15-primary-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"schema15-primary-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"primary":{"type":"keep_basis"}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema15-redo-inherited","to_revision":"schema15-primary-import","presentation":{"before":{"basis":{"width":768,"height":320,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null}},"after":{"basis":{"width":768,"height":320,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}}},"nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["second-qualified-clip","hold-1","source-negative","source-positive"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["schema15-primary-clip","second-qualified-clip","hold-1","source-negative","source-positive"]}}},"schema15-primary-clip":{"before":null,"after":{"label":"Inserted cfr-bframes.mp4","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"schema15-primary-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"schema15-primary-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}}}}},"assets":{"schema15-primary-camera":{"before":null,"after":{"label":"Qualified cfr-bframes.mp4","content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema15-primary-import","to_revision":"schema15-redo-inherited","presentation":{"before":{"basis":{"width":768,"height":320,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}},"after":{"basis":{"width":768,"height":320,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null}}},"nodes":{"acceptance-probe-root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["schema15-primary-clip","second-qualified-clip","hold-1","source-negative","source-positive"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["second-qualified-clip","hold-1","source-negative","source-positive"]}}},"schema15-primary-clip":{"before":{"label":"Inserted cfr-bframes.mp4","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"schema15-primary-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"schema15-primary-camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}}},"after":null}},"assets":{"schema15-primary-camera":{"before":{"label":"Qualified cfr-bframes.mp4","content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"},"after":null}},"marks":{},"overrides":{}},"changed_ids":["acceptance-probe-root","schema15-primary-clip"],"duration_delta":120,"description":"Import source media"}');
INSERT INTO "history" VALUES(30,29,'schema15-primary-geometry','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema15-primary-import","new_revision":"schema15-primary-geometry","command":{"command":"adopt_primary_geometry","width":320,"height":180}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema15-primary-import","to_revision":"schema15-primary-geometry","presentation":{"before":{"basis":{"width":768,"height":320,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}},"after":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"primary_source","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}}},"nodes":{},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema15-primary-geometry","to_revision":"schema15-primary-import","presentation":{"before":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"primary_source","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}},"after":{"basis":{"width":768,"height":320,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}}},"nodes":{},"assets":{},"marks":{},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Adopt primary source geometry"}');
INSERT INTO "history" VALUES(31,30,'schema15-canvas','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"schema15-primary-geometry","new_revision":"schema15-canvas","command":{"command":"set_canvas","width":1280,"height":720}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema15-primary-geometry","to_revision":"schema15-canvas","presentation":{"before":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"primary_source","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}},"after":{"basis":{"width":1280,"height":720,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}}},"nodes":{},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"schema15-canvas","to_revision":"schema15-primary-geometry","presentation":{"before":{"basis":{"width":1280,"height":720,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}},"after":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"primary_source","primary":{"asset":"schema15-primary-camera","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}}},"nodes":{},"assets":{},"marks":{},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Change canvas geometry"}');
CREATE TABLE hold_request_clocks (
    hold_id TEXT PRIMARY KEY,
    high_water INTEGER NOT NULL
        CHECK (high_water BETWEEN 1 AND 9223372036854775807)
) STRICT;
INSERT INTO "hold_request_clocks" VALUES('hold-1',1);
CREATE TABLE original_media (
        content_id TEXT PRIMARY KEY,
        version INTEGER NOT NULL CHECK(version > 0),
        record TEXT NOT NULL CHECK(json_valid(record))
    ) STRICT;
INSERT INTO "original_media" VALUES('blake3:ea0f6c331dca8f5eb0f56c91fe99bb5bdbc74f5b07082cdee070ac57d11b08a7',1,'{"object":{"content":{"algorithm":"blake3","digest":"ea0f6c331dca8f5eb0f56c91fe99bb5bdbc74f5b07082cdee070ac57d11b08a7"},"byte_length":32832},"sha256":[231,154,186,144,234,180,45,192,87,23,27,229,43,233,236,183,148,195,226,121,12,87,49,120,50,112,236,231,60,36,210,238],"label":"pcm-stereo-48000.wav","version":1,"managed":true,"linked":null}');
INSERT INTO "original_media" VALUES('blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1',1,'{"object":{"content":{"algorithm":"blake3","digest":"16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1"},"byte_length":39157},"sha256":[90,130,10,121,191,85,13,72,77,142,203,55,255,221,208,72,208,99,103,148,235,206,93,184,62,79,92,229,245,230,73,24],"label":"cfr-bframes.mp4","version":1,"managed":true,"linked":null}');
INSERT INTO "original_media" VALUES('blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7',1,'{"object":{"content":{"algorithm":"blake3","digest":"bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7"},"byte_length":43455},"sha256":[63,208,125,0,170,96,69,84,220,83,137,83,12,210,97,172,137,97,173,237,99,97,66,111,176,11,245,81,154,198,72,132],"label":"vfr.mp4","version":1,"managed":true,"linked":null}');
CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
INSERT INTO "redo" VALUES(1,31);
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('revision-1',NULL,'initial','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "revision-1",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('accepted','revision-1','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "accepted",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "generated",
            "accepted": {
              "artifact": {
                "sampled_asset": "accepted-sampled",
                "sampled_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"
                  },
                  "byte_length": 5450240
                },
                "native_asset": "accepted-native",
                "native_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"
                  },
                  "byte_length": 4544157
                },
                "provenance": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"
                  },
                  "byte_length": 45014
                },
                "sampling": {
                  "schema_version": 1,
                  "project_rate": {
                    "numerator": 30000,
                    "denominator": 1001
                  },
                  "native_rate": {
                    "numerator": 24,
                    "denominator": 1
                  },
                  "native_frame_count": 25,
                  "output_frame_count": 30,
                  "interpolation": "encoded_srgb_rgb8_linear_half_up"
                }
              },
              "fallback": {
                "type": "background"
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('undo-accept','accepted','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "undo-accept",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('redo-accept','undo-accept','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "redo-accept",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "generated",
            "accepted": {
              "artifact": {
                "sampled_asset": "accepted-sampled",
                "sampled_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"
                  },
                  "byte_length": 5450240
                },
                "native_asset": "accepted-native",
                "native_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"
                  },
                  "byte_length": 4544157
                },
                "provenance": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"
                  },
                  "byte_length": 45014
                },
                "sampling": {
                  "schema_version": 1,
                  "project_rate": {
                    "numerator": 30000,
                    "denominator": 1001
                  },
                  "native_rate": {
                    "numerator": 24,
                    "denominator": 1
                  },
                  "native_frame_count": 25,
                  "output_frame_count": 30,
                  "interpolation": "encoded_srgb_rgb8_linear_half_up"
                }
              },
              "fallback": {
                "type": "background"
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('revert-to-fallback','redo-accept','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "revert-to-fallback",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema10-add-av-asset','revert-to-fallback','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema10-add-av-asset",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema10-insert-negative','schema10-add-av-asset','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema10-insert-negative",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "source-negative",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema10-insert-positive','schema10-insert-negative','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema10-insert-positive",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "source-negative",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema10-source-mark','schema10-insert-positive','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema10-source-mark",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "source-negative",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema10-local-mark','schema10-source-mark','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema10-local-mark",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "source-negative",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema10-rename-source','schema10-local-mark','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema10-rename-source",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Renamed source with negative offset",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema10-undo-source','schema10-rename-source','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema10-undo-source",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "source-negative",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema10-redo-source','schema10-undo-source','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema10-redo-source",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Renamed source with negative offset",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema10-pending-redo','schema10-redo-source','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema10-pending-redo",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "source-negative",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-redo-inherited','schema10-pending-redo','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-redo-inherited",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Renamed source with negative offset",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-audio-negative','schema11-redo-inherited','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-audio-negative",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Renamed source with negative offset",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "fit_beat"
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-audio-positive','schema11-audio-negative','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-audio-positive",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Renamed source with negative offset",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-local-mark','schema11-audio-positive','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-local-mark",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Renamed source with negative offset",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-abandoned-rename','schema11-local-mark','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-abandoned-rename",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Abandoned schema 11 branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-undo-abandoned','schema11-abandoned-rename','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-undo-abandoned",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Renamed source with negative offset",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-rename-branch','schema11-undo-abandoned','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-rename-branch",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 11 independent audio",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-undo-branch','schema11-rename-branch','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-undo-branch",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Renamed source with negative offset",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-redo-branch','schema11-undo-branch','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-redo-branch",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 11 independent audio",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema11-pending-redo','schema11-redo-branch','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema11-pending-redo",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Renamed source with negative offset",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-redo-inherited','schema11-pending-redo','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-redo-inherited",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 11 independent audio",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-video-negative','schema12-redo-inherited','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-video-negative",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 11 independent audio",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-video-positive','schema12-video-negative','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-video-positive",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 11 independent audio",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-local-mark','schema12-video-positive','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-local-mark",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 11 independent audio",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-abandoned-rename','schema12-local-mark','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-abandoned-rename",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Abandoned schema 12 branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-undo-abandoned','schema12-abandoned-rename','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-undo-abandoned",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 11 independent audio",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-rename-branch','schema12-undo-abandoned','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-rename-branch",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-undo-branch','schema12-rename-branch','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-undo-branch",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 11 independent audio",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-redo-branch','schema12-undo-branch','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-redo-branch",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema12-pending-redo','schema12-redo-branch','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema12-pending-redo",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 11 independent audio",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-redo-inherited','schema12-pending-redo','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-redo-inherited",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-place-video-negative','schema13-redo-inherited','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-place-video-negative",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-place-audio-negative','schema13-place-video-negative','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-place-audio-negative",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-place-video-positive','schema13-place-audio-negative','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-place-video-positive",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "duration",
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-place-audio-positive','schema13-place-video-positive','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-place-audio-positive",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-local-mark','schema13-place-audio-positive','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-local-mark",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-abandoned-rename','schema13-local-mark','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-abandoned-rename",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Abandoned schema 13 branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-undo-abandoned','schema13-abandoned-rename','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-undo-abandoned",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-rename-branch','schema13-undo-abandoned','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-rename-branch",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-undo-branch','schema13-rename-branch','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-undo-branch",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-redo-branch','schema13-undo-branch','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-redo-branch",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema13-pending-redo','schema13-redo-branch','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema13-pending-redo",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 12 independent streams",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema14-redo-inherited','schema13-pending-redo','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema14-redo-inherited",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema14-first-import','schema14-redo-inherited','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema14-first-import",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "first-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "first-qualified-clip": {
      "label": "Inserted cfr-bframes.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 120120,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 192192,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified cfr-bframes.mp4",
      "content_hash": "blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 120120,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 192192,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema14-undo-first-import','schema14-first-import','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema14-undo-first-import",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema14-second-import','schema14-undo-first-import','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema14-second-import",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "second-qualified-clip": {
      "label": "Inserted vfr.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema14-rename','schema14-second-import','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema14-rename",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "second-qualified-clip": {
      "label": "Schema 14 qualified branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema14-undo-rename','schema14-rename','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema14-undo-rename",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "second-qualified-clip": {
      "label": "Inserted vfr.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema14-redo-rename','schema14-undo-rename','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema14-redo-rename",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "second-qualified-clip": {
      "label": "Schema 14 qualified branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema14-pending-redo','schema14-redo-rename','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema14-pending-redo",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "second-qualified-clip": {
      "label": "Inserted vfr.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema15-redo-inherited','schema14-pending-redo','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema15-redo-inherited",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "second-qualified-clip": {
      "label": "Schema 14 qualified branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema15-primary-import','schema15-redo-inherited','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema15-primary-import",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "schema15-primary-camera",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "schema15-primary-clip",
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "schema15-primary-clip": {
      "label": "Inserted cfr-bframes.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 120120,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 192192,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "second-qualified-clip": {
      "label": "Schema 14 qualified branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    },
    "schema15-primary-camera": {
      "label": "Qualified cfr-bframes.mp4",
      "content_hash": "blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 120120,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 192192,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema15-primary-geometry','schema15-primary-import','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema15-primary-geometry",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "schema15-primary-camera",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "schema15-primary-clip",
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "schema15-primary-clip": {
      "label": "Inserted cfr-bframes.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 120120,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 192192,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "second-qualified-clip": {
      "label": "Schema 14 qualified branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    },
    "schema15-primary-camera": {
      "label": "Qualified cfr-bframes.mp4",
      "content_hash": "blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 120120,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 192192,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema15-canvas','schema15-primary-geometry','edit','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema15-canvas",
  "presentation_basis": {
    "width": 1280,
    "height": 720,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "schema15-primary-camera",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "schema15-primary-clip",
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "schema15-primary-clip": {
      "label": "Inserted cfr-bframes.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 120120,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 192192,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "second-qualified-clip": {
      "label": "Schema 14 qualified branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    },
    "schema15-primary-camera": {
      "label": "Qualified cfr-bframes.mp4",
      "content_hash": "blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 120120,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 192192,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema15-undo-canvas','schema15-canvas','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema15-undo-canvas",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "schema15-primary-camera",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "schema15-primary-clip",
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "schema15-primary-clip": {
      "label": "Inserted cfr-bframes.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 120120,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 192192,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "second-qualified-clip": {
      "label": "Schema 14 qualified branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    },
    "schema15-primary-camera": {
      "label": "Qualified cfr-bframes.mp4",
      "content_hash": "blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 120120,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 192192,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema15-redo-canvas','schema15-undo-canvas','redo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema15-redo-canvas",
  "presentation_basis": {
    "width": 1280,
    "height": 720,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "schema15-primary-camera",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "schema15-primary-clip",
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "schema15-primary-clip": {
      "label": "Inserted cfr-bframes.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 120120,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 192192,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "second-qualified-clip": {
      "label": "Schema 14 qualified branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    },
    "schema15-primary-camera": {
      "label": "Qualified cfr-bframes.mp4",
      "content_hash": "blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 120120,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 192192,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('schema15-pending-redo','schema15-redo-canvas','undo','{
  "schema_version": 10,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "schema15-pending-redo",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "schema15-primary-camera",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "schema15-primary-clip",
          "second-qualified-clip",
          "hold-1",
          "source-negative",
          "source-positive"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "schema15-primary-clip": {
      "label": "Inserted cfr-bframes.mp4",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 120120,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "schema15-primary-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 192192,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "second-qualified-clip": {
      "label": "Schema 14 qualified branch",
      "kind": {
        "type": "source",
        "source": {
          "duration": 240,
          "video": {
            "type": "stream",
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 238238,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "238",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "qualified-camera",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 384384,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "240",
              "denominator": "1"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "source-negative": {
      "label": "Schema 13 signed stream placements",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "28750",
              "denominator": "1001"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-1",
              "denominator": "147"
            },
            "frames": {
              "numerator": "60000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": -137
        }
      }
    },
    "source-positive": {
      "label": "source-positive",
      "kind": {
        "type": "source",
        "source": {
          "duration": 60,
          "video": {
            "type": "stream",
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              },
              "end": {
                "ticks": 4000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 1000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "-3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            },
            "endpoints": "reject"
          },
          "audio": {
            "asset": "original-av",
            "span": {
              "start": {
                "ticks": -48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 144000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "3",
              "denominator": "7"
            },
            "frames": {
              "numerator": "120000",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 2401
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    },
    "original-av": {
      "label": "Original A/V with distinct origins",
      "content_hash": "7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 4000,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": -48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 144000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120
    },
    "qualified-camera": {
      "label": "Qualified vfr.mp4",
      "content_hash": "blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 238238,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 384384,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6"
    },
    "schema15-primary-camera": {
      "label": "Qualified cfr-bframes.mp4",
      "content_hash": "blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 120120,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 192192,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "marks": {
    "local-mark": {
      "owner": "source-positive",
      "label": "Exact local boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-positive",
          "position": {
            "numerator": "7",
            "denominator": "3"
          }
        },
        "bias": "left"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "picture-mapping-local-mark": {
      "owner": "source-negative",
      "label": "Exact picture mapping boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "placement-local-mark": {
      "owner": "source-negative",
      "label": "Exact signed placement boundary",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "source-negative",
          "position": {
            "numerator": "13",
            "denominator": "7"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "source-audio-mark": {
      "owner": "source-negative",
      "label": "Original audio zero",
      "boundary": {
        "coordinate": {
          "space": "source",
          "asset": "original-av",
          "moment": {
            "type": "audio_sample",
            "sample": 0,
            "sample_rate": 48000
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
CREATE TABLE source_qualifications (
            id TEXT PRIMARY KEY,
            original_content_id TEXT NOT NULL REFERENCES original_media(content_id),
            original_ref TEXT NOT NULL CHECK(json_valid(original_ref)),
            snapshot BLOB NOT NULL
        ) STRICT;
INSERT INTO "source_qualifications" VALUES('c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6','blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1','{"content":{"algorithm":"blake3","digest":"16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1"},"byte_length":39157}',X'7B22736368656D615F76657273696F6E223A312C226465636F6465725F636F6E7472616374223A2266666D7065672D382E302E332F736F757263652D6465636F6465642D7631222C2274696D696E675F706F6C6963795F76657273696F6E223A312C22636F6E74656E74223A7B22736861323536223A5B39302C3133302C31302C3132312C3139312C38352C31332C37322C37372C3134322C3230332C35352C3235352C3232312C3230382C37322C3230382C39392C3130332C3134382C3233352C3230362C39332C3138342C36322C37392C39322C3232392C3234352C3233302C37332C32345D2C22627974655F6C656E677468223A33393135377D2C226F726967696E5F7365636F6E6473223A7B226E756D657261746F72223A2230222C2264656E6F6D696E61746F72223A2231227D2C22766964656F223A7B22696E646578223A7B22736368656D615F76657273696F6E223A312C22636F6E74656E74223A7B22736861323536223A5B39302C3133302C31302C3132312C3139312C38352C31332C37322C37372C3134322C3230332C35352C3235352C3232312C3230382C37322C3230382C39392C3130332C3134382C3233352C3230362C39332C3138342C36322C37392C39322C3232392C3234352C3233302C37332C32345D2C22627974655F6C656E677468223A33393135377D2C2273747265616D5F696E646578223A302C22696E646578223A7B226173736574223A227175616C69666965642D736F75726365222C2274696D655F62617365223A7B226E756D657261746F72223A312C2264656E6F6D696E61746F72223A33303030307D2C226672616D6573223A5B7B226964656E74697479223A302C22707473223A302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A307D2C7B226964656E74697479223A312C22707473223A313030312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A313030317D2C7B226964656E74697479223A322C22707473223A323030322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A323030327D2C7B226964656E74697479223A332C22707473223A333030332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A333030337D2C7B226964656E74697479223A342C22707473223A343030342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A343030347D2C7B226964656E74697479223A352C22707473223A353030352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A353030357D2C7B226964656E74697479223A362C22707473223A363030362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A363030367D2C7B226964656E74697479223A372C22707473223A373030372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A373030377D2C7B226964656E74697479223A382C22707473223A383030382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A383030387D2C7B226964656E74697479223A392C22707473223A393030392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A393030397D2C7B226964656E74697479223A31302C22707473223A31303031302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31303031307D2C7B226964656E74697479223A31312C22707473223A31313031312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31313031317D2C7B226964656E74697479223A31322C22707473223A31323031322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31323031327D2C7B226964656E74697479223A31332C22707473223A31333031332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31333031337D2C7B226964656E74697479223A31342C22707473223A31343031342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31343031347D2C7B226964656E74697479223A31352C22707473223A31353031352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31353031357D2C7B226964656E74697479223A31362C22707473223A31363031362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31363031367D2C7B226964656E74697479223A31372C22707473223A31373031372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31373031377D2C7B226964656E74697479223A31382C22707473223A31383031382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31383031387D2C7B226964656E74697479223A31392C22707473223A31393031392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31393031397D2C7B226964656E74697479223A32302C22707473223A32303032302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32303032307D2C7B226964656E74697479223A32312C22707473223A32313032312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32313032317D2C7B226964656E74697479223A32322C22707473223A32323032322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32323032327D2C7B226964656E74697479223A32332C22707473223A32333032332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32333032337D2C7B226964656E74697479223A32342C22707473223A32343032342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32343032347D2C7B226964656E74697479223A32352C22707473223A32353032352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32353032357D2C7B226964656E74697479223A32362C22707473223A32363032362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32363032367D2C7B226964656E74697479223A32372C22707473223A32373032372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32373032377D2C7B226964656E74697479223A32382C22707473223A32383032382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32383032387D2C7B226964656E74697479223A32392C22707473223A32393032392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32393032397D2C7B226964656E74697479223A33302C22707473223A33303033302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33303033307D2C7B226964656E74697479223A33312C22707473223A33313033312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33313033317D2C7B226964656E74697479223A33322C22707473223A33323033322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33323033327D2C7B226964656E74697479223A33332C22707473223A33333033332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33333033337D2C7B226964656E74697479223A33342C22707473223A33343033342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33343033347D2C7B226964656E74697479223A33352C22707473223A33353033352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33353033357D2C7B226964656E74697479223A33362C22707473223A33363033362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33363033367D2C7B226964656E74697479223A33372C22707473223A33373033372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33373033377D2C7B226964656E74697479223A33382C22707473223A33383033382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33383033387D2C7B226964656E74697479223A33392C22707473223A33393033392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33393033397D2C7B226964656E74697479223A34302C22707473223A34303034302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34303034307D2C7B226964656E74697479223A34312C22707473223A34313034312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34313034317D2C7B226964656E74697479223A34322C22707473223A34323034322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34323034327D2C7B226964656E74697479223A34332C22707473223A34333034332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34333034337D2C7B226964656E74697479223A34342C22707473223A34343034342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34343034347D2C7B226964656E74697479223A34352C22707473223A34353034352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34353034357D2C7B226964656E74697479223A34362C22707473223A34363034362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34363034367D2C7B226964656E74697479223A34372C22707473223A34373034372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34373034377D2C7B226964656E74697479223A34382C22707473223A34383034382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34383034387D2C7B226964656E74697479223A34392C22707473223A34393034392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34393034397D2C7B226964656E74697479223A35302C22707473223A35303035302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35303035307D2C7B226964656E74697479223A35312C22707473223A35313035312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35313035317D2C7B226964656E74697479223A35322C22707473223A35323035322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35323035327D2C7B226964656E74697479223A35332C22707473223A35333035332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35333035337D2C7B226964656E74697479223A35342C22707473223A35343035342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35343035347D2C7B226964656E74697479223A35352C22707473223A35353035352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35353035357D2C7B226964656E74697479223A35362C22707473223A35363035362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35363035367D2C7B226964656E74697479223A35372C22707473223A35373035372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35373035377D2C7B226964656E74697479223A35382C22707473223A35383035382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35383035387D2C7B226964656E74697479223A35392C22707473223A35393035392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35393035397D2C7B226964656E74697479223A36302C22707473223A36303036302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36303036307D2C7B226964656E74697479223A36312C22707473223A36313036312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36313036317D2C7B226964656E74697479223A36322C22707473223A36323036322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36323036327D2C7B226964656E74697479223A36332C22707473223A36333036332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36333036337D2C7B226964656E74697479223A36342C22707473223A36343036342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36343036347D2C7B226964656E74697479223A36352C22707473223A36353036352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36353036357D2C7B226964656E74697479223A36362C22707473223A36363036362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36363036367D2C7B226964656E74697479223A36372C22707473223A36373036372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36373036377D2C7B226964656E74697479223A36382C22707473223A36383036382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36383036387D2C7B226964656E74697479223A36392C22707473223A36393036392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36393036397D2C7B226964656E74697479223A37302C22707473223A37303037302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37303037307D2C7B226964656E74697479223A37312C22707473223A37313037312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37313037317D2C7B226964656E74697479223A37322C22707473223A37323037322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37323037327D2C7B226964656E74697479223A37332C22707473223A37333037332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37333037337D2C7B226964656E74697479223A37342C22707473223A37343037342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37343037347D2C7B226964656E74697479223A37352C22707473223A37353037352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37353037357D2C7B226964656E74697479223A37362C22707473223A37363037362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37363037367D2C7B226964656E74697479223A37372C22707473223A37373037372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37373037377D2C7B226964656E74697479223A37382C22707473223A37383037382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37383037387D2C7B226964656E74697479223A37392C22707473223A37393037392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37393037397D2C7B226964656E74697479223A38302C22707473223A38303038302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38303038307D2C7B226964656E74697479223A38312C22707473223A38313038312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38313038317D2C7B226964656E74697479223A38322C22707473223A38323038322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38323038327D2C7B226964656E74697479223A38332C22707473223A38333038332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38333038337D2C7B226964656E74697479223A38342C22707473223A38343038342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38343038347D2C7B226964656E74697479223A38352C22707473223A38353038352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38353038357D2C7B226964656E74697479223A38362C22707473223A38363038362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38363038367D2C7B226964656E74697479223A38372C22707473223A38373038372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38373038377D2C7B226964656E74697479223A38382C22707473223A38383038382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38383038387D2C7B226964656E74697479223A38392C22707473223A38393038392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38393038397D2C7B226964656E74697479223A39302C22707473223A39303039302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39303039307D2C7B226964656E74697479223A39312C22707473223A39313039312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39313039317D2C7B226964656E74697479223A39322C22707473223A39323039322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39323039327D2C7B226964656E74697479223A39332C22707473223A39333039332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39333039337D2C7B226964656E74697479223A39342C22707473223A39343039342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39343039347D2C7B226964656E74697479223A39352C22707473223A39353039352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39353039357D2C7B226964656E74697479223A39362C22707473223A39363039362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39363039367D2C7B226964656E74697479223A39372C22707473223A39373039372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39373039377D2C7B226964656E74697479223A39382C22707473223A39383039382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39383039387D2C7B226964656E74697479223A39392C22707473223A39393039392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39393039397D2C7B226964656E74697479223A3130302C22707473223A3130303130302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130303130307D2C7B226964656E74697479223A3130312C22707473223A3130313130312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130313130317D2C7B226964656E74697479223A3130322C22707473223A3130323130322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130323130327D2C7B226964656E74697479223A3130332C22707473223A3130333130332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130333130337D2C7B226964656E74697479223A3130342C22707473223A3130343130342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130343130347D2C7B226964656E74697479223A3130352C22707473223A3130353130352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130353130357D2C7B226964656E74697479223A3130362C22707473223A3130363130362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130363130367D2C7B226964656E74697479223A3130372C22707473223A3130373130372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130373130377D2C7B226964656E74697479223A3130382C22707473223A3130383130382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130383130387D2C7B226964656E74697479223A3130392C22707473223A3130393130392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130393130397D2C7B226964656E74697479223A3131302C22707473223A3131303131302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131303131307D2C7B226964656E74697479223A3131312C22707473223A3131313131312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131313131317D2C7B226964656E74697479223A3131322C22707473223A3131323131322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131323131327D2C7B226964656E74697479223A3131332C22707473223A3131333131332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131333131337D2C7B226964656E74697479223A3131342C22707473223A3131343131342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131343131347D2C7B226964656E74697479223A3131352C22707473223A3131353131352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131353131357D2C7B226964656E74697479223A3131362C22707473223A3131363131362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131363131367D2C7B226964656E74697479223A3131372C22707473223A3131373131372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131373131377D2C7B226964656E74697479223A3131382C22707473223A3131383131382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131383131387D2C7B226964656E74697479223A3131392C22707473223A3131393131392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A6E756C6C7D5D2C227465726D696E616C5F656E64223A3132303132302C227465726D696E616C5F70726F76656E616E6365223A226465636F6465645F6672616D655F6475726174696F6E227D7D2C22696E746572707265746174696F6E223A7B227769647468223A3332302C22686569676874223A3138302C2273747265616D5F696E646578223A302C2274696D655F626173655F6E756D223A312C2274696D655F626173655F64656E223A33303030302C2273616D706C655F6173706563745F6E756D223A312C2273616D706C655F6173706563745F64656E223A312C22726F746174696F6E5F717561727465725F7475726E73223A302C22636F6C6F72223A7B2272616E6765223A226C696D69746564222C226D6174726978223A226274373039222C227472616E73666572223A226274373039222C227072696D6172696573223A226274373039227D2C22636F646563223A2268323634222C22706978656C5F666F726D6174223A2279757634323070222C2273747265616D5F7374617274223A302C2273747265616D5F6475726174696F6E223A3132303132302C22636F6E7461696E65725F7374617274223A6E756C6C2C22636F6E7461696E65725F6475726174696F6E223A6E756C6C2C22617564696F5F73747265616D73223A5B7B2273747265616D5F696E646578223A312C22636F646563223A22616163222C2274696D655F626173655F6E756D223A312C2274696D655F626173655F64656E223A34383030302C2273747265616D5F7374617274223A302C2273747265616D5F6475726174696F6E223A3139323139322C2273616D706C655F72617465223A34383030302C226368616E6E656C5F636F756E74223A327D5D7D7D2C22617564696F223A7B22736368656D615F76657273696F6E223A312C226465636F6465725F636F6E7472616374223A2266666D7065672D382E302E332F617564696F2D6D616E75616C2D736B69702D7631222C22636F6E74656E74223A7B22736861323536223A5B39302C3133302C31302C3132312C3139312C38352C31332C37322C37372C3134322C3230332C35352C3235352C3232312C3230382C37322C3230382C39392C3130332C3134382C3233352C3230362C39332C3138342C36322C37392C39322C3232392C3234352C3233302C37332C32345D2C22627974655F6C656E677468223A33393135377D2C2273747265616D223A7B2273747265616D5F696E646578223A312C22636F646563223A22616163222C2274696D655F62617365223A7B226E756D657261746F72223A312C2264656E6F6D696E61746F72223A34383030307D2C2273616D706C655F72617465223A34383030302C226368616E6E656C5F6C61796F7574223A7B226F72646572223A226E6174697665222C226368616E6E656C73223A322C226D61736B223A337D2C2273747265616D5F7374617274223A302C2273747265616D5F6475726174696F6E223A3139323139322C22696E697469616C5F70616464696E67223A302C22747261696C696E675F70616464696E67223A302C227365656B5F707265726F6C6C223A307D2C226F62736572766174696F6E73223A5B7B22707473223A2D313032342C2264697363617264223A747275652C226465636F64655F74696D657374616D70223A2D313032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A7B226C656164696E67223A313032342C22747261696C696E67223A302C226C656164696E675F726561736F6E223A302C22747261696C696E675F726561736F6E223A307D7D2C7B22707473223A302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A313032342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A313032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A323034382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A323034382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A333037322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A333037322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A343039362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A343039362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A353132302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A353132302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A363134342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A363134342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A373136382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A373136382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A383139322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A383139322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A393231362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A393231362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31303234302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31303234302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31313236342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31313236342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31323238382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31323238382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31333331322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31333331322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31343333362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31343333362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31353336302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31353336302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31363338342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31363338342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31373430382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31373430382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31383433322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31383433322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31393435362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31393435362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32303438302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32303438302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32313530342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32313530342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32323532382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32323532382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32333535322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32333535322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32343537362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32343537362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32353630302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32353630302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32363632342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32363632342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32373634382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32373634382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32383637322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32383637322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32393639362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32393639362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33303732302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33303732302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33313734342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33313734342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33323736382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33323736382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33333739322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33333739322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33343831362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33343831362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33353834302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33353834302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33363836342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33363836342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33373838382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33373838382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33383931322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33383931322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33393933362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33393933362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34303936302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34303936302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34313938342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34313938342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34333030382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34333030382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34343033322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34343033322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34353035362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34353035362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34363038302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34363038302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34373130342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34373130342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34383132382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34383132382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34393135322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34393135322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35303137362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35303137362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35313230302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35313230302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35323232342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35323232342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35333234382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35333234382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35343237322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35343237322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35353239362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35353239362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35363332302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35363332302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35373334342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35373334342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35383336382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35383336382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35393339322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35393339322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36303431362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36303431362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36313434302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36313434302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36323436342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36323436342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36333438382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36333438382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36343531322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36343531322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36353533362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36353533362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36363536302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36363536302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36373538342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36373538342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36383630382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36383630382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36393633322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36393633322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37303635362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37303635362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37313638302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37313638302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37323730342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37323730342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37333732382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37333732382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37343735322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37343735322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37353737362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37353737362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37363830302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37363830302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37373832342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37373832342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37383834382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37383834382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37393837322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37393837322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38303839362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38303839362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38313932302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38313932302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38323934342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38323934342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38333936382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38333936382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38343939322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38343939322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38363031362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38363031362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38373034302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38373034302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38383036342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38383036342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38393038382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38393038382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39303131322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39303131322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39313133362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39313133362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39323136302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39323136302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39333138342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39333138342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39343230382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39343230382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39353233322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39353233322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39363235362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39363235362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39373238302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39373238302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39383330342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39383330342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39393332382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39393332382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130303335322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130303335322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130313337362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130313337362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130323430302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130323430302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130333432342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130333432342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130343434382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130343434382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130353437322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130353437322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130363439362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130363439362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130373532302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130373532302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130383534342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130383534342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130393536382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130393536382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131303539322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131303539322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131313631362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131313631362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131323634302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131323634302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131333636342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131333636342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131343638382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131343638382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131353731322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131353731322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131363733362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131363733362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131373736302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131373736302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131383738342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131383738342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131393830382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131393830382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132303833322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132303833322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132313835362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132313835362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132323838302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132323838302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132333930342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132333930342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132343932382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132343932382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132353935322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132353935322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132363937362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132363937362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132383030302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132383030302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132393032342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132393032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133303034382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133303034382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133313037322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133313037322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133323039362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133323039362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133333132302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133333132302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133343134342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133343134342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133353136382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133353136382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133363139322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133363139322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133373231362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133373231362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133383234302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133383234302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133393236342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133393236342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134303238382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134303238382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134313331322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134313331322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134323333362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134323333362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134333336302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134333336302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134343338342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134343338342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134353430382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134353430382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134363433322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134363433322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134373435362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134373435362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134383438302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134383438302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134393530342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134393530342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135303532382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135303532382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135313535322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135313535322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135323537362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135323537362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135333630302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135333630302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135343632342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135343632342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135353634382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135353634382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135363637322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135363637322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135373639362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135373639362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135383732302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135383732302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135393734342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135393734342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136303736382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136303736382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136313739322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136313739322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136323831362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136323831362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136333834302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136333834302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136343836342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136343836342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136353838382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136353838382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136363931322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136363931322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136373933362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136373933362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136383936302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136383936302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136393938342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136393938342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137313030382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137313030382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137323033322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137323033322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137333035362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137333035362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137343038302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137343038302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137353130342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137353130342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137363132382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137363132382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137373135322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137373135322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137383137362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137383137362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137393230302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137393230302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138303232342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138303232342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138313234382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138313234382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138323237322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138323237322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138333239362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138333239362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138343332302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138343332302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138353334342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138353334342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138363336382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138363336382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138373339322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138373339322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138383431362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138383431362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138393434302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138393434302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139303436342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139303436342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139313438382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139313438382C227265706F727465645F6475726174696F6E223A3730342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D5D7D7D');
INSERT INTO "source_qualifications" VALUES('9877217b0ec8fa0200644d68aa0b3e4620ba6520eec13ad75a33613d548256b6','blake3:bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7','{"content":{"algorithm":"blake3","digest":"bea5617533ed23db2f9d07cd7d85e8994f910fc4d283cacf45af90b7295629d7"},"byte_length":43455}',X'7B22736368656D615F76657273696F6E223A312C226465636F6465725F636F6E7472616374223A2266666D7065672D382E302E332F736F757263652D6465636F6465642D7631222C2274696D696E675F706F6C6963795F76657273696F6E223A312C22636F6E74656E74223A7B22736861323536223A5B36332C3230382C3132352C302C3137302C39362C36392C38342C3232302C38332C3133372C38332C31322C3231302C39372C3137322C3133372C39372C3137332C3233372C39392C39372C36362C3131312C3137362C31312C3234352C38312C3135342C3139382C37322C3133325D2C22627974655F6C656E677468223A34333435357D2C226F726967696E5F7365636F6E6473223A7B226E756D657261746F72223A2230222C2264656E6F6D696E61746F72223A2231227D2C22766964656F223A7B22696E646578223A7B22736368656D615F76657273696F6E223A312C22636F6E74656E74223A7B22736861323536223A5B36332C3230382C3132352C302C3137302C39362C36392C38342C3232302C38332C3133372C38332C31322C3231302C39372C3137322C3133372C39372C3137332C3233372C39392C39372C36362C3131312C3137362C31312C3234352C38312C3135342C3139382C37322C3133325D2C22627974655F6C656E677468223A34333435357D2C2273747265616D5F696E646578223A302C22696E646578223A7B226173736574223A227175616C69666965642D736F75726365222C2274696D655F62617365223A7B226E756D657261746F72223A312C2264656E6F6D696E61746F72223A33303030307D2C226672616D6573223A5B7B226964656E74697479223A302C22707473223A302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A307D2C7B226964656E74697479223A312C22707473223A313030312C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A313030317D2C7B226964656E74697479223A322C22707473223A333030332C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A333030337D2C7B226964656E74697479223A332C22707473223A363030362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A363030367D2C7B226964656E74697479223A342C22707473223A373030372C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A373030377D2C7B226964656E74697479223A352C22707473223A393030392C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A393030397D2C7B226964656E74697479223A362C22707473223A31323031322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31323031327D2C7B226964656E74697479223A372C22707473223A31333031332C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31333031337D2C7B226964656E74697479223A382C22707473223A31353031352C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31353031357D2C7B226964656E74697479223A392C22707473223A31383031382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31383031387D2C7B226964656E74697479223A31302C22707473223A31393031392C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31393031397D2C7B226964656E74697479223A31312C22707473223A32313032312C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A32313032317D2C7B226964656E74697479223A31322C22707473223A32343032342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A32343032347D2C7B226964656E74697479223A31332C22707473223A32353032352C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A32353032357D2C7B226964656E74697479223A31342C22707473223A32373032372C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A32373032377D2C7B226964656E74697479223A31352C22707473223A33303033302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A33303033307D2C7B226964656E74697479223A31362C22707473223A33313033312C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A33313033317D2C7B226964656E74697479223A31372C22707473223A33333033332C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A33333033337D2C7B226964656E74697479223A31382C22707473223A33363033362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A33363033367D2C7B226964656E74697479223A31392C22707473223A33373033372C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A33373033377D2C7B226964656E74697479223A32302C22707473223A33393033392C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A33393033397D2C7B226964656E74697479223A32312C22707473223A34323034322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A34323034327D2C7B226964656E74697479223A32322C22707473223A34333034332C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A34333034337D2C7B226964656E74697479223A32332C22707473223A34353034352C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A34353034357D2C7B226964656E74697479223A32342C22707473223A34383034382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A34383034387D2C7B226964656E74697479223A32352C22707473223A34393034392C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A34393034397D2C7B226964656E74697479223A32362C22707473223A35313035312C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A35313035317D2C7B226964656E74697479223A32372C22707473223A35343035342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A35343035347D2C7B226964656E74697479223A32382C22707473223A35353035352C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A35353035357D2C7B226964656E74697479223A32392C22707473223A35373035372C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A35373035377D2C7B226964656E74697479223A33302C22707473223A36303036302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A36303036307D2C7B226964656E74697479223A33312C22707473223A36313036312C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A36313036317D2C7B226964656E74697479223A33322C22707473223A36333036332C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A36333036337D2C7B226964656E74697479223A33332C22707473223A36363036362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A36363036367D2C7B226964656E74697479223A33342C22707473223A36373036372C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A36373036377D2C7B226964656E74697479223A33352C22707473223A36393036392C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A36393036397D2C7B226964656E74697479223A33362C22707473223A37323037322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A37323037327D2C7B226964656E74697479223A33372C22707473223A37333037332C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A37333037337D2C7B226964656E74697479223A33382C22707473223A37353037352C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A37353037357D2C7B226964656E74697479223A33392C22707473223A37383037382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A37383037387D2C7B226964656E74697479223A34302C22707473223A37393037392C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A37393037397D2C7B226964656E74697479223A34312C22707473223A38313038312C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A38313038317D2C7B226964656E74697479223A34322C22707473223A38343038342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A38343038347D2C7B226964656E74697479223A34332C22707473223A38353038352C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A38353038357D2C7B226964656E74697479223A34342C22707473223A38373038372C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A38373038377D2C7B226964656E74697479223A34352C22707473223A39303039302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A39303039307D2C7B226964656E74697479223A34362C22707473223A39313039312C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A39313039317D2C7B226964656E74697479223A34372C22707473223A39333039332C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A39333039337D2C7B226964656E74697479223A34382C22707473223A39363039362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A39363039367D2C7B226964656E74697479223A34392C22707473223A39373039372C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A39373039377D2C7B226964656E74697479223A35302C22707473223A39393039392C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A39393039397D2C7B226964656E74697479223A35312C22707473223A3130323130322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130323130327D2C7B226964656E74697479223A35322C22707473223A3130333130332C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130333130337D2C7B226964656E74697479223A35332C22707473223A3130353130352C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130353130357D2C7B226964656E74697479223A35342C22707473223A3130383130382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130383130387D2C7B226964656E74697479223A35352C22707473223A3130393130392C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130393130397D2C7B226964656E74697479223A35362C22707473223A3131313131312C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131313131317D2C7B226964656E74697479223A35372C22707473223A3131343131342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131343131347D2C7B226964656E74697479223A35382C22707473223A3131353131352C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131353131357D2C7B226964656E74697479223A35392C22707473223A3131373131372C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131373131377D2C7B226964656E74697479223A36302C22707473223A3132303132302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132303132307D2C7B226964656E74697479223A36312C22707473223A3132313132312C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132313132317D2C7B226964656E74697479223A36322C22707473223A3132333132332C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132333132337D2C7B226964656E74697479223A36332C22707473223A3132363132362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132363132367D2C7B226964656E74697479223A36342C22707473223A3132373132372C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132373132377D2C7B226964656E74697479223A36352C22707473223A3132393132392C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132393132397D2C7B226964656E74697479223A36362C22707473223A3133323133322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133323133327D2C7B226964656E74697479223A36372C22707473223A3133333133332C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133333133337D2C7B226964656E74697479223A36382C22707473223A3133353133352C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133353133357D2C7B226964656E74697479223A36392C22707473223A3133383133382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133383133387D2C7B226964656E74697479223A37302C22707473223A3133393133392C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133393133397D2C7B226964656E74697479223A37312C22707473223A3134313134312C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3134313134317D2C7B226964656E74697479223A37322C22707473223A3134343134342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3134343134347D2C7B226964656E74697479223A37332C22707473223A3134353134352C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3134353134357D2C7B226964656E74697479223A37342C22707473223A3134373134372C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3134373134377D2C7B226964656E74697479223A37352C22707473223A3135303135302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3135303135307D2C7B226964656E74697479223A37362C22707473223A3135313135312C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3135313135317D2C7B226964656E74697479223A37372C22707473223A3135333135332C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3135333135337D2C7B226964656E74697479223A37382C22707473223A3135363135362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3135363135367D2C7B226964656E74697479223A37392C22707473223A3135373135372C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3135373135377D2C7B226964656E74697479223A38302C22707473223A3135393135392C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3135393135397D2C7B226964656E74697479223A38312C22707473223A3136323136322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3136323136327D2C7B226964656E74697479223A38322C22707473223A3136333136332C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3136333136337D2C7B226964656E74697479223A38332C22707473223A3136353136352C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3136353136357D2C7B226964656E74697479223A38342C22707473223A3136383136382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3136383136387D2C7B226964656E74697479223A38352C22707473223A3136393136392C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3136393136397D2C7B226964656E74697479223A38362C22707473223A3137313137312C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3137313137317D2C7B226964656E74697479223A38372C22707473223A3137343137342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3137343137347D2C7B226964656E74697479223A38382C22707473223A3137353137352C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3137353137357D2C7B226964656E74697479223A38392C22707473223A3137373137372C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3137373137377D2C7B226964656E74697479223A39302C22707473223A3138303138302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3138303138307D2C7B226964656E74697479223A39312C22707473223A3138313138312C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3138313138317D2C7B226964656E74697479223A39322C22707473223A3138333138332C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3138333138337D2C7B226964656E74697479223A39332C22707473223A3138363138362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3138363138367D2C7B226964656E74697479223A39342C22707473223A3138373138372C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3138373138377D2C7B226964656E74697479223A39352C22707473223A3138393138392C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3138393138397D2C7B226964656E74697479223A39362C22707473223A3139323139322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3139323139327D2C7B226964656E74697479223A39372C22707473223A3139333139332C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3139333139337D2C7B226964656E74697479223A39382C22707473223A3139353139352C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3139353139357D2C7B226964656E74697479223A39392C22707473223A3139383139382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3139383139387D2C7B226964656E74697479223A3130302C22707473223A3139393139392C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3139393139397D2C7B226964656E74697479223A3130312C22707473223A3230313230312C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3230313230317D2C7B226964656E74697479223A3130322C22707473223A3230343230342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3230343230347D2C7B226964656E74697479223A3130332C22707473223A3230353230352C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3230353230357D2C7B226964656E74697479223A3130342C22707473223A3230373230372C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3230373230377D2C7B226964656E74697479223A3130352C22707473223A3231303231302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3231303231307D2C7B226964656E74697479223A3130362C22707473223A3231313231312C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3231313231317D2C7B226964656E74697479223A3130372C22707473223A3231333231332C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3231333231337D2C7B226964656E74697479223A3130382C22707473223A3231363231362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3231363231367D2C7B226964656E74697479223A3130392C22707473223A3231373231372C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3231373231377D2C7B226964656E74697479223A3131302C22707473223A3231393231392C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3231393231397D2C7B226964656E74697479223A3131312C22707473223A3232323232322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3232323232327D2C7B226964656E74697479223A3131322C22707473223A3232333232332C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3232333232337D2C7B226964656E74697479223A3131332C22707473223A3232353232352C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3232353232357D2C7B226964656E74697479223A3131342C22707473223A3232383232382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3232383232387D2C7B226964656E74697479223A3131352C22707473223A3232393232392C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3232393232397D2C7B226964656E74697479223A3131362C22707473223A3233313233312C227265706F727465645F6475726174696F6E223A333030332C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3233313233317D2C7B226964656E74697479223A3131372C22707473223A3233343233342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3233343233347D2C7B226964656E74697479223A3131382C22707473223A3233353233352C227265706F727465645F6475726174696F6E223A323030322C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3233353233357D2C7B226964656E74697479223A3131392C22707473223A3233373233372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3233373233377D5D2C227465726D696E616C5F656E64223A3233383233382C227465726D696E616C5F70726F76656E616E6365223A226465636F6465645F6672616D655F6475726174696F6E227D7D2C22696E746572707265746174696F6E223A7B227769647468223A3332302C22686569676874223A3138302C2273747265616D5F696E646578223A302C2274696D655F626173655F6E756D223A312C2274696D655F626173655F64656E223A33303030302C2273616D706C655F6173706563745F6E756D223A312C2273616D706C655F6173706563745F64656E223A312C22726F746174696F6E5F717561727465725F7475726E73223A302C22636F6C6F72223A7B2272616E6765223A226C696D69746564222C226D6174726978223A226274373039222C227472616E73666572223A226274373039222C227072696D6172696573223A226274373039227D2C22636F646563223A2268323634222C22706978656C5F666F726D6174223A2279757634323070222C2273747265616D5F7374617274223A302C2273747265616D5F6475726174696F6E223A3233383233382C22636F6E7461696E65725F7374617274223A6E756C6C2C22636F6E7461696E65725F6475726174696F6E223A6E756C6C2C22617564696F5F73747265616D73223A5B7B2273747265616D5F696E646578223A312C22636F646563223A22616163222C2274696D655F626173655F6E756D223A312C2274696D655F626173655F64656E223A34383030302C2273747265616D5F7374617274223A302C2273747265616D5F6475726174696F6E223A3338343338342C2273616D706C655F72617465223A34383030302C226368616E6E656C5F636F756E74223A327D5D7D7D2C22617564696F223A7B22736368656D615F76657273696F6E223A312C226465636F6465725F636F6E7472616374223A2266666D7065672D382E302E332F617564696F2D6D616E75616C2D736B69702D7631222C22636F6E74656E74223A7B22736861323536223A5B36332C3230382C3132352C302C3137302C39362C36392C38342C3232302C38332C3133372C38332C31322C3231302C39372C3137322C3133372C39372C3137332C3233372C39392C39372C36362C3131312C3137362C31312C3234352C38312C3135342C3139382C37322C3133325D2C22627974655F6C656E677468223A34333435357D2C2273747265616D223A7B2273747265616D5F696E646578223A312C22636F646563223A22616163222C2274696D655F62617365223A7B226E756D657261746F72223A312C2264656E6F6D696E61746F72223A34383030307D2C2273616D706C655F72617465223A34383030302C226368616E6E656C5F6C61796F7574223A7B226F72646572223A226E6174697665222C226368616E6E656C73223A322C226D61736B223A337D2C2273747265616D5F7374617274223A302C2273747265616D5F6475726174696F6E223A3338343338342C22696E697469616C5F70616464696E67223A302C22747261696C696E675F70616464696E67223A302C227365656B5F707265726F6C6C223A307D2C226F62736572766174696F6E73223A5B7B22707473223A2D313032342C2264697363617264223A747275652C226465636F64655F74696D657374616D70223A2D313032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A7B226C656164696E67223A313032342C22747261696C696E67223A302C226C656164696E675F726561736F6E223A302C22747261696C696E675F726561736F6E223A307D7D2C7B22707473223A302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A313032342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A313032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A323034382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A323034382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A333037322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A333037322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A343039362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A343039362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A353132302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A353132302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A363134342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A363134342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A373136382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A373136382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A383139322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A383139322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A393231362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A393231362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31303234302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31303234302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31313236342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31313236342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31323238382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31323238382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31333331322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31333331322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31343333362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31343333362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31353336302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31353336302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31363338342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31363338342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31373430382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31373430382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31383433322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31383433322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31393435362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31393435362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32303438302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32303438302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32313530342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32313530342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32323532382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32323532382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32333535322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32333535322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32343537362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32343537362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32353630302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32353630302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32363632342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32363632342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32373634382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32373634382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32383637322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32383637322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32393639362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32393639362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33303732302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33303732302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33313734342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33313734342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33323736382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33323736382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33333739322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33333739322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33343831362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33343831362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33353834302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33353834302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33363836342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33363836342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33373838382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33373838382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33383931322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33383931322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33393933362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33393933362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34303936302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34303936302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34313938342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34313938342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34333030382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34333030382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34343033322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34343033322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34353035362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34353035362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34363038302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34363038302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34373130342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34373130342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34383132382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34383132382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34393135322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34393135322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35303137362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35303137362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35313230302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35313230302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35323232342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35323232342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35333234382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35333234382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35343237322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35343237322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35353239362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35353239362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35363332302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35363332302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35373334342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35373334342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35383336382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35383336382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35393339322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35393339322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36303431362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36303431362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36313434302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36313434302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36323436342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36323436342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36333438382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36333438382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36343531322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36343531322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36353533362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36353533362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36363536302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36363536302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36373538342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36373538342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36383630382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36383630382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36393633322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36393633322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37303635362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37303635362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37313638302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37313638302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37323730342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37323730342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37333732382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37333732382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37343735322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37343735322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37353737362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37353737362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37363830302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37363830302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37373832342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37373832342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37383834382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37383834382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37393837322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37393837322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38303839362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38303839362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38313932302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38313932302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38323934342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38323934342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38333936382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38333936382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38343939322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38343939322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38363031362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38363031362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38373034302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38373034302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38383036342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38383036342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38393038382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38393038382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39303131322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39303131322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39313133362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39313133362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39323136302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39323136302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39333138342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39333138342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39343230382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39343230382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39353233322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39353233322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39363235362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39363235362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39373238302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39373238302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39383330342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39383330342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39393332382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39393332382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130303335322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130303335322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130313337362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130313337362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130323430302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130323430302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130333432342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130333432342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130343434382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130343434382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130353437322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130353437322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130363439362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130363439362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130373532302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130373532302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130383534342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130383534342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130393536382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130393536382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131303539322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131303539322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131313631362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131313631362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131323634302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131323634302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131333636342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131333636342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131343638382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131343638382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131353731322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131353731322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131363733362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131363733362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131373736302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131373736302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131383738342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131383738342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131393830382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131393830382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132303833322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132303833322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132313835362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132313835362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132323838302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132323838302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132333930342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132333930342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132343932382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132343932382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132353935322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132353935322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132363937362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132363937362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132383030302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132383030302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132393032342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132393032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133303034382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133303034382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133313037322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133313037322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133323039362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133323039362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133333132302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133333132302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133343134342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133343134342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133353136382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133353136382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133363139322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133363139322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133373231362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133373231362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133383234302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133383234302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133393236342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133393236342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134303238382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134303238382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134313331322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134313331322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134323333362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134323333362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134333336302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134333336302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134343338342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134343338342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134353430382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134353430382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134363433322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134363433322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134373435362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134373435362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134383438302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134383438302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134393530342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134393530342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135303532382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135303532382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135313535322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135313535322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135323537362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135323537362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135333630302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135333630302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135343632342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135343632342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135353634382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135353634382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135363637322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135363637322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135373639362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135373639362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135383732302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135383732302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135393734342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135393734342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136303736382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136303736382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136313739322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136313739322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136323831362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136323831362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136333834302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136333834302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136343836342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136343836342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136353838382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136353838382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136363931322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136363931322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136373933362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136373933362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136383936302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136383936302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136393938342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136393938342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137313030382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137313030382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137323033322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137323033322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137333035362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137333035362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137343038302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137343038302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137353130342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137353130342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137363132382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137363132382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137373135322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137373135322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137383137362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137383137362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137393230302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137393230302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138303232342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138303232342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138313234382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138313234382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138323237322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138323237322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138333239362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138333239362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138343332302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138343332302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138353334342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138353334342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138363336382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138363336382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138373339322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138373339322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138383431362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138383431362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138393434302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138393434302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139303436342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139303436342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139313438382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139313438382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139323531322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139323531322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139333533362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139333533362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139343536302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139343536302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139353538342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139353538342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139363630382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139363630382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139373633322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139373633322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139383635362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139383635362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139393638302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139393638302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230303730342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230303730342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230313732382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230313732382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230323735322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230323735322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230333737362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230333737362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230343830302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230343830302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230353832342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230353832342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230363834382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230363834382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230373837322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230373837322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230383839362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230383839362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230393932302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230393932302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231303934342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231303934342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231313936382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231313936382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231323939322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231323939322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231343031362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231343031362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231353034302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231353034302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231363036342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231363036342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231373038382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231373038382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231383131322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231383131322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231393133362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231393133362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232303136302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232303136302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232313138342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232313138342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232323230382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232323230382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232333233322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232333233322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232343235362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232343235362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232353238302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232353238302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232363330342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232363330342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232373332382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232373332382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232383335322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232383335322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232393337362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232393337362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233303430302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233303430302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233313432342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233313432342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233323434382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233323434382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233333437322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233333437322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233343439362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233343439362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233353532302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233353532302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233363534342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233363534342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233373536382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233373536382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233383539322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233383539322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233393631362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233393631362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234303634302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234303634302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234313636342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234313636342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234323638382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234323638382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234333731322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234333731322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234343733362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234343733362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234353736302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234353736302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234363738342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234363738342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234373830382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234373830382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234383833322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234383833322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234393835362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234393835362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235303838302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235303838302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235313930342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235313930342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235323932382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235323932382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235333935322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235333935322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235343937362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235343937362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235363030302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235363030302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235373032342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235373032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235383034382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235383034382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235393037322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235393037322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236303039362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236303039362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236313132302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236313132302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236323134342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236323134342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236333136382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236333136382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236343139322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236343139322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236353231362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236353231362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236363234302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236363234302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236373236342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236373236342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236383238382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236383238382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236393331322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236393331322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237303333362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237303333362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237313336302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237313336302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237323338342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237323338342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237333430382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237333430382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237343433322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237343433322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237353435362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237353435362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237363438302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237363438302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237373530342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237373530342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237383532382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237383532382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237393535322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237393535322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238303537362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238303537362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238313630302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238313630302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238323632342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238323632342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238333634382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238333634382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238343637322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238343637322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238353639362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238353639362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238363732302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238363732302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238373734342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238373734342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238383736382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238383736382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238393739322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238393739322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3239303831362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3239303831362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3239313834302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3239313834302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3239323836342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3239323836342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3239333838382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3239333838382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3239343931322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3239343931322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3239353933362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3239353933362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3239363936302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3239363936302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3239373938342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3239373938342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3239393030382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3239393030382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330303033322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330303033322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330313035362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330313035362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330323038302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330323038302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330333130342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330333130342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330343132382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330343132382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330353135322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330353135322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330363137362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330363137362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330373230302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330373230302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330383232342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330383232342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3330393234382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3330393234382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331303237322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331303237322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331313239362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331313239362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331323332302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331323332302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331333334342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331333334342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331343336382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331343336382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331353339322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331353339322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331363431362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331363431362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331373434302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331373434302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331383436342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331383436342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3331393438382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3331393438382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332303531322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332303531322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332313533362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332313533362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332323536302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332323536302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332333538342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332333538342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332343630382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332343630382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332353633322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332353633322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332363635362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332363635362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332373638302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332373638302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332383730342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332383730342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3332393732382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3332393732382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333303735322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333303735322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333313737362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333313737362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333323830302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333323830302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333333832342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333333832342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333343834382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333343834382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333353837322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333353837322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333363839362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333363839362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333373932302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333373932302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333383934342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333383934342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3333393936382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3333393936382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3334303939322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3334303939322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3334323031362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3334323031362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3334333034302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3334333034302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3334343036342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3334343036342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3334353038382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3334353038382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3334363131322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3334363131322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3334373133362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3334373133362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3334383136302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3334383136302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3334393138342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3334393138342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335303230382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335303230382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335313233322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335313233322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335323235362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335323235362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335333238302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335333238302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335343330342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335343330342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335353332382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335353332382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335363335322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335363335322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335373337362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335373337362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335383430302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335383430302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3335393432342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3335393432342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336303434382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336303434382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336313437322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336313437322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336323439362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336323439362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336333532302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336333532302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336343534342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336343534342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336353536382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336353536382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336363539322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336363539322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336373631362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336373631362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336383634302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336383634302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3336393636342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3336393636342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337303638382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337303638382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337313731322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337313731322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337323733362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337323733362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337333736302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337333736302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337343738342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337343738342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337353830382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337353830382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337363833322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337363833322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337373835362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337373835362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337383838302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337383838302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3337393930342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3337393930342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3338303932382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3338303932382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3338313935322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3338313935322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3338323937362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3338323937362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3338343030302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3338343030302C227265706F727465645F6475726174696F6E223A3338342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D5D7D7D');
CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id)
        ) STRICT;
INSERT INTO "state" VALUES(1,'schema15-pending-redo',30);
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
