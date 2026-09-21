-- Genuine schema-12 project generated and validated with commit
-- 35b8011e775049af41ef5e12c40fa49e62467cef (core schema 7).
-- Starts from v11-history.sql, migrated by a CLI rebuilt from that revision.
-- The same revision's ProjectStore API redoes the inherited edit, applies exact
-- video Duration mappings 28750/1001 and 120000/1001, preserving audio mappings
-- through direct and occurrence commands, adds a mark, and retains an abandoned
-- rename branch plus undo/redo chronology ending with one pending redo.
-- Existing generated requests, attempts, receipts, and originals are preserved.
-- Generated with tools/media-qualification/evidence/2026-09-21-import-timing/
-- fixture/generate.py; this dump comes from SQLite's backup API. Media bytes
-- remain external to this metadata-only migration fixture.
PRAGMA application_id=1146113585;
PRAGMA user_version=12;
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
CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
INSERT INTO "redo" VALUES(1,18);
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('revision-1',NULL,'initial','{
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
  "schema_version": 7,
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
CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id)
        ) STRICT;
INSERT INTO "state" VALUES(1,'schema12-pending-redo',16);
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
