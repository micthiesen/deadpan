-- Authentic schema 39 / core schema 33 snapshot. No authored SQL modifications.
-- SQLite backup/dump retained before the render-job schema change at 097f735.
-- Source inventory: 8332c9d07a73d66364e6ff1468399a369355cefedcb7b68de5d91db4231c790d.
-- Fixture: generated; full producer/snapshot evidence retained with render-job qualification.
PRAGMA application_id=1146113585;
PRAGMA user_version=39;
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
INSERT INTO "generation_attempt_heads" VALUES('request',1,'attempt','attempt');
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
INSERT INTO "generation_attempts" VALUES('request','attempt',1,'cancel','ready','inference',5,NULL,'{"native":{"reference":"outputs/native.mp4","sha256":"b16a7ab4aa925c1f432075b7cb6b28e7b38fc1e5187ef7cbec07d400dd756492","byte_length":27226},"provenance":{"reference":"outputs/provenance.json","sha256":"8433b7b75fde53dfd51ce442db53e66f4af225c40619e23b18f53ab23359501c","byte_length":4090},"video":{"frames":25,"frame_rate":{"numerator":24,"denominator":1},"width":4,"height":2},"provider":{"pack_id":"fixture","pack_version":"1","runtime_id":"fixture","runtime_version":"1","seed":1}}',NULL,NULL,NULL);
CREATE TABLE generation_bundle_receipts (
    request_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    bundle TEXT NOT NULL CHECK (json_valid(bundle)),
    availability TEXT NOT NULL CHECK (availability IN ('present','evicted')),
    PRIMARY KEY (request_id,attempt_id),
    FOREIGN KEY (request_id,attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;
INSERT INTO "generation_bundle_receipts" VALUES('request','attempt','{"native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"provenance_object":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"native_video":{"frames":25,"frame_rate":{"numerator":24,"denominator":1},"width":4,"height":2},"sampled_video":{"frames":30,"frame_rate":{"numerator":30000,"denominator":1001},"width":4,"height":2},"plan":{"schema_version":1,"operation":"bridge","interpolation":"linear","project":{"interior_frames":30,"frame_rate":{"numerator":30000,"denominator":1001}},"native":{"frame_count":25,"frame_rate":{"numerator":24,"denominator":1},"width":4,"height":2},"timing":{"requested_boundary_duration":{"numerator":"31031","denominator":"30000"},"actual_boundary_duration":{"numerator":"1","denominator":"1"},"retime_deviation":{"numerator":"-1031","denominator":"30000"}},"sampling":{"endpoint_policy":"interior_only"}},"provider":{"pack_id":"fixture","pack_version":"1","runtime_id":"fixture","runtime_version":"1","seed":1},"native_sha256":"b16a7ab4aa925c1f432075b7cb6b28e7b38fc1e5187ef7cbec07d400dd756492","native_byte_length":27226,"provenance_sha256":"8433b7b75fde53dfd51ce442db53e66f4af225c40619e23b18f53ab23359501c","provenance_byte_length":4090,"validator":{"id":"native-ffv1","version":"bridge-3"},"availability":"present","admission":{"native_span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"sampled_span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"inputs":{"context_sha256":"087e6eb5830c38cc015baacdfa470a44536bb18027aa8ad67f061caa82c7c3e7","manifest":{"content":{"algorithm":"blake3","digest":"fd9f059e30d63516595380346d8085c060faa62193694c41880aa5f4e488dee4"},"byte_length":1209},"left":{"content":{"algorithm":"blake3","digest":"cb5c62af599fef79ae342ff2fc8c2ea05128bcdeb1abc5dc5ddb8cd89555fd23"},"byte_length":19},"right":{"content":{"algorithm":"blake3","digest":"09397c98b599a4e78f0f1441606892738c34a8a027cce901edc9fa750fc11bcf"},"byte_length":20}}}}','present');
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
INSERT INTO "generation_requests" VALUES('request','project','hold',1,'revision','087e6eb5830c38cc015baacdfa470a44536bb18027aa8ad67f061caa82c7c3e7','{"video":{"frames":30,"frame_rate":{"numerator":30000,"denominator":1001},"width":4,"height":2},"conditioning":"bridge","motion":"still"}','{"pack_id":"fixture","pack_version":"1","runtime_id":"fixture","runtime_version":"1","seed":1}','{"schema_version":1,"operation":"bridge","interpolation":"linear","project":{"interior_frames":30,"frame_rate":{"numerator":30000,"denominator":1001}},"native":{"frame_count":25,"frame_rate":{"numerator":24,"denominator":1},"width":4,"height":2},"timing":{"requested_boundary_duration":{"numerator":"31031","denominator":"30000"},"actual_boundary_duration":{"numerator":"1","denominator":"1"},"retime_deviation":{"numerator":"-1031","denominator":"30000"}},"sampling":{"endpoint_policy":"interior_only"}}','stale');
CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
INSERT INTO "history" VALUES(1,NULL,'accepted','{"project_id":"project","expected_revision":"revision","new_revision":"accepted","command":{"command":"accept_generated_hold","node":"hold","artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"assets":{"native":{"label":"Generated 43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4","content_hash":"blake3:43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":25},"sampled":{"label":"Generated 19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b","content_hash":"blake3:19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":30}}}}','{"forward":{"project_id":"project","from_revision":"revision","to_revision":"accepted","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{"native":{"before":null,"after":{"label":"Generated 43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4","content_hash":"blake3:43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":25}},"sampled":{"before":null,"after":{"label":"Generated 19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b","content_hash":"blake3:19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":30}}},"marks":{},"overrides":{}},"inverse":{"project_id":"project","from_revision":"accepted","to_revision":"revision","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{"native":{"before":{"label":"Generated 43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4","content_hash":"blake3:43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":25},"after":null},"sampled":{"before":{"label":"Generated 19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b","content_hash":"blake3:19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":30},"after":null}},"marks":{},"overrides":{}},"changed_ids":["hold"],"duration_delta":0,"description":"Accept generated hold"}');
INSERT INTO "history" VALUES(2,1,'shorter-accepted-prefix','{"project_id":"project","expected_revision":"redo-accept","new_revision":"shorter-accepted-prefix","command":{"command":"set_hold_duration","node":"hold","duration":12}}','{"forward":{"project_id":"project","from_revision":"redo-accept","to_revision":"shorter-accepted-prefix","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":12,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project","from_revision":"shorter-accepted-prefix","to_revision":"redo-accept","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":12,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["hold"],"duration_delta":-18,"description":"Change hold duration"}');
INSERT INTO "history" VALUES(3,2,'restored-accepted-prefix','{"project_id":"project","expected_revision":"shorter-accepted-prefix","new_revision":"restored-accepted-prefix","command":{"command":"set_hold_duration","node":"hold","duration":30}}','{"forward":{"project_id":"project","from_revision":"shorter-accepted-prefix","to_revision":"restored-accepted-prefix","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":12,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project","from_revision":"restored-accepted-prefix","to_revision":"shorter-accepted-prefix","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":12,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["hold"],"duration_delta":18,"description":"Change hold duration"}');
INSERT INTO "history" VALUES(4,3,'reverted','{"project_id":"project","expected_revision":"restored-accepted-prefix","new_revision":"reverted","command":{"command":"revert_generated_hold","node":"hold"}}','{"forward":{"project_id":"project","from_revision":"restored-accepted-prefix","to_revision":"reverted","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project","from_revision":"reverted","to_revision":"restored-accepted-prefix","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":960,"height":540,"fit":"fill","layers":[{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}},null]}]},"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"},"byte_length":4041},"native_asset":"native","native_object":{"content":{"algorithm":"blake3","digest":"43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"},"byte_length":3485},"provenance":{"content":{"algorithm":"blake3","digest":"1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"},"byte_length":9149},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["hold"],"duration_delta":0,"description":"Revert generated hold"}');
CREATE TABLE hold_request_clocks (
    hold_id TEXT PRIMARY KEY,
    high_water INTEGER NOT NULL
        CHECK (high_water BETWEEN 1 AND 9223372036854775807)
) STRICT;
INSERT INTO "hold_request_clocks" VALUES('hold',1);
CREATE TABLE original_media (
        content_id TEXT PRIMARY KEY,
        version INTEGER NOT NULL CHECK(version > 0),
        record TEXT NOT NULL CHECK(json_valid(record))
    ) STRICT;
CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
INSERT INTO "redo" VALUES(1,4);
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('revision',NULL,'initial','{
  "schema_version": 33,
  "project_id": "project",
  "revision_id": "revision",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
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
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 960,
                "height": 540,
                "fit": "fill",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "center_y": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "scale": {
                      "numerator": "2",
                      "denominator": "1"
                    }
                  },
                  null
                ]
              }
            ]
          },
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
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('accepted','revision','edit','{
  "schema_version": 33,
  "project_id": "project",
  "revision_id": "accepted",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
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
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 960,
                "height": 540,
                "fit": "fill",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "center_y": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "scale": {
                      "numerator": "2",
                      "denominator": "1"
                    }
                  },
                  null
                ]
              }
            ]
          },
          "duration": 30,
          "video": {
            "type": "generated",
            "accepted": {
              "artifact": {
                "sampled_asset": "sampled",
                "sampled_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"
                  },
                  "byte_length": 4041
                },
                "native_asset": "native",
                "native_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"
                  },
                  "byte_length": 3485
                },
                "provenance": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"
                  },
                  "byte_length": 9149
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
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    }
  },
  "assets": {
    "native": {
      "label": "Generated 43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
      "content_hash": "blake3:43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
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
    "sampled": {
      "label": "Generated 19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
      "content_hash": "blake3:19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
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
  "schema_version": 33,
  "project_id": "project",
  "revision_id": "undo-accept",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
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
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 960,
                "height": 540,
                "fit": "fill",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "center_y": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "scale": {
                      "numerator": "2",
                      "denominator": "1"
                    }
                  },
                  null
                ]
              }
            ]
          },
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
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('redo-accept','undo-accept','redo','{
  "schema_version": 33,
  "project_id": "project",
  "revision_id": "redo-accept",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
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
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 960,
                "height": 540,
                "fit": "fill",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "center_y": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "scale": {
                      "numerator": "2",
                      "denominator": "1"
                    }
                  },
                  null
                ]
              }
            ]
          },
          "duration": 30,
          "video": {
            "type": "generated",
            "accepted": {
              "artifact": {
                "sampled_asset": "sampled",
                "sampled_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"
                  },
                  "byte_length": 4041
                },
                "native_asset": "native",
                "native_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"
                  },
                  "byte_length": 3485
                },
                "provenance": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"
                  },
                  "byte_length": 9149
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
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    }
  },
  "assets": {
    "native": {
      "label": "Generated 43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
      "content_hash": "blake3:43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
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
    "sampled": {
      "label": "Generated 19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
      "content_hash": "blake3:19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
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
INSERT INTO "revisions" VALUES('shorter-accepted-prefix','redo-accept','edit','{
  "schema_version": 33,
  "project_id": "project",
  "revision_id": "shorter-accepted-prefix",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
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
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 960,
                "height": 540,
                "fit": "fill",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "center_y": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "scale": {
                      "numerator": "2",
                      "denominator": "1"
                    }
                  },
                  null
                ]
              }
            ]
          },
          "duration": 12,
          "video": {
            "type": "generated",
            "accepted": {
              "artifact": {
                "sampled_asset": "sampled",
                "sampled_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"
                  },
                  "byte_length": 4041
                },
                "native_asset": "native",
                "native_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"
                  },
                  "byte_length": 3485
                },
                "provenance": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"
                  },
                  "byte_length": 9149
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
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    }
  },
  "assets": {
    "native": {
      "label": "Generated 43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
      "content_hash": "blake3:43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
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
    "sampled": {
      "label": "Generated 19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
      "content_hash": "blake3:19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
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
INSERT INTO "revisions" VALUES('restored-accepted-prefix','shorter-accepted-prefix','edit','{
  "schema_version": 33,
  "project_id": "project",
  "revision_id": "restored-accepted-prefix",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
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
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 960,
                "height": 540,
                "fit": "fill",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "center_y": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "scale": {
                      "numerator": "2",
                      "denominator": "1"
                    }
                  },
                  null
                ]
              }
            ]
          },
          "duration": 30,
          "video": {
            "type": "generated",
            "accepted": {
              "artifact": {
                "sampled_asset": "sampled",
                "sampled_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"
                  },
                  "byte_length": 4041
                },
                "native_asset": "native",
                "native_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"
                  },
                  "byte_length": 3485
                },
                "provenance": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"
                  },
                  "byte_length": 9149
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
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    }
  },
  "assets": {
    "native": {
      "label": "Generated 43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
      "content_hash": "blake3:43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
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
    "sampled": {
      "label": "Generated 19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
      "content_hash": "blake3:19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
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
INSERT INTO "revisions" VALUES('reverted','restored-accepted-prefix','edit','{
  "schema_version": 33,
  "project_id": "project",
  "revision_id": "reverted",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
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
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 960,
                "height": 540,
                "fit": "fill",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "center_y": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "scale": {
                      "numerator": "2",
                      "denominator": "1"
                    }
                  },
                  null
                ]
              }
            ]
          },
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
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    }
  },
  "assets": {
    "native": {
      "label": "Generated 43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
      "content_hash": "blake3:43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
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
    "sampled": {
      "label": "Generated 19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
      "content_hash": "blake3:19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
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
INSERT INTO "revisions" VALUES('ui-generated-ready','reverted','undo','{
  "schema_version": 33,
  "project_id": "project",
  "revision_id": "ui-generated-ready",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
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
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 960,
                "height": 540,
                "fit": "fill",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "center_y": {
                      "numerator": "1",
                      "denominator": "2"
                    },
                    "scale": {
                      "numerator": "2",
                      "denominator": "1"
                    }
                  },
                  null
                ]
              }
            ]
          },
          "duration": 30,
          "video": {
            "type": "generated",
            "accepted": {
              "artifact": {
                "sampled_asset": "sampled",
                "sampled_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b"
                  },
                  "byte_length": 4041
                },
                "native_asset": "native",
                "native_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4"
                  },
                  "byte_length": 3485
                },
                "provenance": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "1f0836c622de3bfa44e770b6cd897b65950ba8d8b6985c5d30f78b7e01b4423e"
                  },
                  "byte_length": 9149
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
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    }
  },
  "assets": {
    "native": {
      "label": "Generated 43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
      "content_hash": "blake3:43a1d3af88b5e3fc3f34d4317802d3c57028188017e8fc95ebdd54ef878b24b4",
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
    "sampled": {
      "label": "Generated 19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
      "content_hash": "blake3:19cb18405c91a593b3be93a9ef9c2e36900ad317e8bc4eaf3e9f2880ce80333b",
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
CREATE TABLE single_source (
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            profile TEXT NOT NULL CHECK(json_valid(profile)),
            baseline_history INTEGER REFERENCES history(id)
        ) STRICT;
CREATE TABLE source_qualifications (
            id TEXT PRIMARY KEY,
            original_content_id TEXT NOT NULL REFERENCES original_media(content_id),
            original_ref TEXT NOT NULL CHECK(json_valid(original_ref)),
            snapshot BLOB NOT NULL
        ) STRICT;
CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id),
            workflow TEXT NOT NULL DEFAULT 'generic' CHECK(workflow IN ('generic','single_source_v1'))
        ) STRICT;
INSERT INTO "state" VALUES(1,'ui-generated-ready',3,'generic');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
