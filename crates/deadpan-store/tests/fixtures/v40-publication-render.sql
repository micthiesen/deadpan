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
CREATE TABLE generation_bundle_receipts (
    request_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    bundle TEXT NOT NULL CHECK (json_valid(bundle)),
    availability TEXT NOT NULL CHECK (availability IN ('present','evicted')),
    PRIMARY KEY (request_id,attempt_id),
    FOREIGN KEY (request_id,attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;
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
CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
INSERT INTO "history" VALUES(1,NULL,'registered','{"project_id":"project-picture-qualification","expected_revision":"initial","new_revision":"registered","command":{"command":"import_source","id":"original","asset":{"label":"Synthetic original","content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"},"insertion":{"parent":"root","index":0,"node":"source","label":"Full Original","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"primary":{"type":"adopt","basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"}}}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"initial","to_revision":"registered","presentation":{"before":{"basis":{"width":1920,"height":1080,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},"state":{"rate_origin":"provisional","geometry_origin":"default","primary":null}},"after":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"primary_source","geometry_origin":"primary_source","primary":{"asset":"original","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}}},"nodes":{"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":[]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["source"]}}},"source":{"before":null,"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}}}}},"assets":{"original":{"before":null,"after":{"label":"Synthetic original","content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"registered","to_revision":"initial","presentation":{"before":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"primary_source","geometry_origin":"primary_source","primary":{"asset":"original","qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"}}},"after":{"basis":{"width":1920,"height":1080,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},"state":{"rate_origin":"provisional","geometry_origin":"default","primary":null}}},"nodes":{"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["source"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":[]}}},"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}}},"after":null}},"assets":{"original":{"before":{"label":"Synthetic original","content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"},"after":null}},"marks":{},"overrides":{}},"changed_ids":["root","source"],"duration_delta":120,"description":"Import source media"}');
INSERT INTO "history" VALUES(2,1,'framed','{"project_id":"project-picture-qualification","expected_revision":"registered","new_revision":"framed","command":{"command":"set_framing","node":"source","framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"3","denominator":"5"},"center_y":{"numerator":"2","denominator":"5"},"scale":{"numerator":"7","denominator":"5"}}}}}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"registered","to_revision":"framed","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"3","denominator":"5"},"center_y":{"numerator":"2","denominator":"5"},"scale":{"numerator":"7","denominator":"5"}}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"framed","to_revision":"registered","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"3","denominator":"5"},"center_y":{"numerator":"2","denominator":"5"},"scale":{"numerator":"7","denominator":"5"}}}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source"],"duration_delta":0,"description":"Change framing"}');
INSERT INTO "history" VALUES(3,2,'fast','{"project_id":"project-picture-qualification","expected_revision":"framed","new_revision":"fast","command":{"command":"wrap_retime","node":"source","id":"fast","duration":60,"pitch":"follow_speed"}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"framed","to_revision":"fast","nodes":{"fast":{"before":null,"after":{"label":"Retime","kind":{"type":"retime","child":"source","duration":60,"mapping":{"start":0,"end":120},"pitch":"follow_speed"}}},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["source"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["fast"]}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"fast","to_revision":"framed","nodes":{"fast":{"before":{"label":"Retime","kind":{"type":"retime","child":"source","duration":60,"mapping":{"start":0,"end":120},"pitch":"follow_speed"}},"after":null},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["fast"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["source"]}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["fast","root"],"duration_delta":-60,"description":"Wrap retime"}');
INSERT INTO "history" VALUES(4,3,'repeat','{"project_id":"project-picture-qualification","expected_revision":"fast","new_revision":"repeat","command":{"command":"wrap_repeat","node":"fast","id":"repeat","plays":2,"gap":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}},"anchor_policy":"first"}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"fast","to_revision":"repeat","nodes":{"repeat":{"before":null,"after":{"label":"Repeat","kind":{"type":"repeat","child":"fast","iterations":{"runs":[{"allocation":"repeat","first":0,"count":2}]},"gap":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["fast"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"repeat","to_revision":"fast","nodes":{"repeat":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"fast","iterations":{"runs":[{"allocation":"repeat","first":0,"count":2}]},"gap":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["fast"]}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["repeat","root"],"duration_delta":62,"description":"Wrap repeat"}');
INSERT INTO "history" VALUES(5,4,'freeze','{"project_id":"project-picture-qualification","expected_revision":"repeat","new_revision":"freeze","command":{"command":"insert","parent":"root","index":1,"subtree":{"root":"freeze","nodes":{"freeze":{"label":"freeze","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":320,"height":180,"fit":"fit","layers":[{"center_x":{"numerator":"3","denominator":"5"},"center_y":{"numerator":"2","denominator":"5"},"scale":{"numerator":"7","denominator":"5"}}]}]},"duration":3,"video":{"type":"freeze","asset":"original","timestamp":{"ticks":41041,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"type":"silence"}}}}},"overrides":{}}}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"repeat","to_revision":"freeze","nodes":{"freeze":{"before":null,"after":{"label":"freeze","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":320,"height":180,"fit":"fit","layers":[{"center_x":{"numerator":"3","denominator":"5"},"center_y":{"numerator":"2","denominator":"5"},"scale":{"numerator":"7","denominator":"5"}}]}]},"duration":3,"video":{"type":"freeze","asset":"original","timestamp":{"ticks":41041,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"type":"silence"}}}}},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["repeat","freeze"]}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"freeze","to_revision":"repeat","nodes":{"freeze":{"before":{"label":"freeze","kind":{"type":"hold","recipe":{"picture_context":{"canvases":[{"width":320,"height":180,"fit":"fit","layers":[{"center_x":{"numerator":"3","denominator":"5"},"center_y":{"numerator":"2","denominator":"5"},"scale":{"numerator":"7","denominator":"5"}}]}]},"duration":3,"video":{"type":"freeze","asset":"original","timestamp":{"ticks":41041,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"type":"silence"}}}},"after":null},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["repeat","freeze"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["freeze","root"],"duration_delta":3,"description":"Insert beats"}');
INSERT INTO "history" VALUES(6,5,'background','{"project_id":"project-picture-qualification","expected_revision":"freeze","new_revision":"background","command":{"command":"insert","parent":"root","index":2,"subtree":{"root":"background","nodes":{"background":{"label":"background","kind":{"type":"hold","recipe":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"overrides":{}}}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"freeze","to_revision":"background","nodes":{"background":{"before":null,"after":{"label":"background","kind":{"type":"hold","recipe":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["repeat","freeze"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["repeat","freeze","background"]}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"background","to_revision":"freeze","nodes":{"background":{"before":{"label":"background","kind":{"type":"hold","recipe":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["repeat","freeze","background"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["repeat","freeze"]}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["background","root"],"duration_delta":3,"description":"Insert beats"}');
INSERT INTO "history" VALUES(7,6,'live-change','{"project_id":"project-picture-qualification","expected_revision":"background","new_revision":"live-change","command":{"command":"set_framing","node":"source","framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}}}}}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"background","to_revision":"live-change","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"3","denominator":"5"},"center_y":{"numerator":"2","denominator":"5"},"scale":{"numerator":"7","denominator":"5"}}}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"live-change","to_revision":"background","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}}}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"3","denominator":"5"},"center_y":{"numerator":"2","denominator":"5"},"scale":{"numerator":"7","denominator":"5"}}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source"],"duration_delta":0,"description":"Change framing"}');
INSERT INTO "history" VALUES(8,7,'worker-live-change','{"project_id":"project-picture-qualification","expected_revision":"live-redo","new_revision":"worker-live-change","command":{"command":"set_framing","node":"source","framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"3"},"center_y":{"numerator":"2","denominator":"3"},"scale":{"numerator":"3","denominator":"2"}}}}}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"live-redo","to_revision":"worker-live-change","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}}}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"3"},"center_y":{"numerator":"2","denominator":"3"},"scale":{"numerator":"3","denominator":"2"}}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"worker-live-change","to_revision":"live-redo","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"3"},"center_y":{"numerator":"2","denominator":"3"},"scale":{"numerator":"3","denominator":"2"}}}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"2","denominator":"1"}}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source"],"duration_delta":0,"description":"Change framing"}');
INSERT INTO "history" VALUES(9,8,'encoded-live-change','{"project_id":"project-picture-qualification","expected_revision":"worker-live-redo","new_revision":"encoded-live-change","command":{"command":"set_framing","node":"source","framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"4"},"center_y":{"numerator":"3","denominator":"4"},"scale":{"numerator":"5","denominator":"4"}}}}}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"worker-live-redo","to_revision":"encoded-live-change","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"3"},"center_y":{"numerator":"2","denominator":"3"},"scale":{"numerator":"3","denominator":"2"}}}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"4"},"center_y":{"numerator":"3","denominator":"4"},"scale":{"numerator":"5","denominator":"4"}}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"encoded-live-change","to_revision":"worker-live-redo","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"4"},"center_y":{"numerator":"3","denominator":"4"},"scale":{"numerator":"5","denominator":"4"}}}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"3"},"center_y":{"numerator":"2","denominator":"3"},"scale":{"numerator":"3","denominator":"2"}}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source"],"duration_delta":0,"description":"Change framing"}');
INSERT INTO "history" VALUES(10,9,'durable-live-edit','{"project_id":"project-picture-qualification","expected_revision":"encoded-live-redo","new_revision":"durable-live-edit","command":{"command":"set_framing","node":"source","framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"3"},"center_y":{"numerator":"2","denominator":"3"},"scale":{"numerator":"3","denominator":"2"}}}}}}','{"forward":{"project_id":"project-picture-qualification","from_revision":"encoded-live-redo","to_revision":"durable-live-edit","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"4"},"center_y":{"numerator":"3","denominator":"4"},"scale":{"numerator":"5","denominator":"4"}}}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"3"},"center_y":{"numerator":"2","denominator":"3"},"scale":{"numerator":"3","denominator":"2"}}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project-picture-qualification","from_revision":"durable-live-edit","to_revision":"encoded-live-redo","nodes":{"source":{"before":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"3"},"center_y":{"numerator":"2","denominator":"3"},"scale":{"numerator":"3","denominator":"2"}}}}},"after":{"label":"Full Original","kind":{"type":"source","source":{"duration":120,"video":{"type":"stream","asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"original","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120","denominator":"1"}},"link":"linked","audio_offset":0}},"framing":{"value":{"type":"static","pose":{"center_x":{"numerator":"1","denominator":"4"},"center_y":{"numerator":"3","denominator":"4"},"scale":{"numerator":"5","denominator":"4"}}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["source"],"duration_delta":0,"description":"Change framing"}');
CREATE TABLE hold_request_clocks (
    hold_id TEXT PRIMARY KEY,
    high_water INTEGER NOT NULL
        CHECK (high_water BETWEEN 1 AND 9223372036854775807)
) STRICT;
CREATE TABLE original_media (
        content_id TEXT PRIMARY KEY,
        version INTEGER NOT NULL CHECK(version > 0),
        record TEXT NOT NULL CHECK(json_valid(record))
    ) STRICT;
INSERT INTO "original_media" VALUES('blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1',1,'{"object":{"content":{"algorithm":"blake3","digest":"16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1"},"byte_length":39157},"sha256":[90,130,10,121,191,85,13,72,77,142,203,55,255,221,208,72,208,99,103,148,235,206,93,184,62,79,92,229,245,230,73,24],"label":"cfr-bframes.mp4","version":1,"managed":true,"linked":null}');
CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
INSERT INTO "redo" VALUES(1,10);
CREATE TABLE render_attempts (
        job_id TEXT NOT NULL REFERENCES render_jobs(job_id), attempt_id TEXT PRIMARY KEY,
        ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 1 AND 9223372036854775807),
        cancellation_token TEXT NOT NULL, state TEXT NOT NULL,
        transition_sequence INTEGER NOT NULL CHECK(transition_sequence BETWEEN 1 AND 9223372036854775807),
        body TEXT NOT NULL CHECK(json_valid(body)), UNIQUE(job_id,ordinal), UNIQUE(job_id,attempt_id)
    ) STRICT;
INSERT INTO "render_attempts" VALUES('durable-structural','structural-encode-1',1,'structural-encode-cancel-1','interrupted',4,'{"job_id":"durable-structural","attempt_id":"structural-encode-1","cancellation_token":"structural-encode-cancel-1","ordinal":1,"transition_sequence":4,"state":"interrupted","checkpoint_attempt_id":"structural-encode-1","cancellation_requested":false,"diagnostic":{"code":"InterruptedOnOpen","detail":"Writable reopen interrupted abandoned render work; retry explicitly. Retained checkpoints remain available."},"verification":null}');
INSERT INTO "render_attempts" VALUES('durable-structural','structural-verify-2',2,'structural-verify-cancel-2','verified',3,'{"job_id":"durable-structural","attempt_id":"structural-verify-2","cancellation_token":"structural-verify-cancel-2","ordinal":2,"transition_sequence":3,"state":"verified","checkpoint_attempt_id":"structural-encode-1","cancellation_requested":false,"diagnostic":null,"verification":{"schema_version":1,"validator_id":"deadpan-sdr-finished-file","validator_version":"0.1.0:policy-1","movie_sha256":"12a48645584a9b83100b996c69e7fd93c2df29f576499afa0b3b27344fa03a79","movie_byte_length":97359,"report":{"audio_edit_media_time":1024,"audio_packets":170,"audio_samples":172973,"contract":{"choice":{"b_frames":"none","mode":"hardware"},"picture":{"canvas":[320,180],"color_policy":"sdr_rec709","frame_count":108,"frame_rate":{"denominator":1001,"numerator":30000},"project_audio_end":205005,"project_audio_start":32032,"project_id":"project-picture-qualification","range":{"end":128,"start":20},"raster":[320,180],"relative_aspect_error":{"denominator":"1","numerator":"0"},"revision_id":"background","terminal_pts":108108,"time_base":{"denominator":30000,"numerator":1}}},"document_sha256":"af8e5672cee86b4d64827ef4e86b8af5ad9b9ca11d5c8b46e18e0f4d7d5663ff","fresh_gop_frames":108,"gops":7,"manual_first_sample":-1024,"manual_physical_samples":174080,"maximum_b_run":0,"movie_bytes":97359,"movie_sha256":"12a48645584a9b83100b996c69e7fd93c2df29f576499afa0b3b27344fa03a79","movie_timescale":240000,"ordinary_first_sample":0,"ordinary_physical_samples":173056,"policy_version":1,"runtime_versions":[4066151,4064103,3934311],"video_edit_media_time":0,"video_frames":108,"video_packets":108}}}');
CREATE TABLE render_candidate_checkpoints (
        job_id TEXT NOT NULL, attempt_id TEXT PRIMARY KEY,
        media TEXT NOT NULL CHECK(json_valid(media)),
        FOREIGN KEY(job_id,attempt_id) REFERENCES render_attempts(job_id,attempt_id)
    ) STRICT;
INSERT INTO "render_candidate_checkpoints" VALUES('durable-structural','structural-encode-1','{"movie":{"content":{"algorithm":"blake3","digest":"cc76ef2ee8f1401489c1618096352c3ceb86bac6fa1b46f0e8bcaa3fe84d192b"},"byte_length":97359},"movie_sha256":"12a48645584a9b83100b996c69e7fd93c2df29f576499afa0b3b27344fa03a79","manifest":{"content":{"algorithm":"blake3","digest":"ac7fa9502a8e8c58e6a0979139948693e4fad5965238b7d50c6daeaa8284a75d"},"byte_length":1931},"manifest_sha256":"b9616711cc6a4fa2eff0a1fdd39901866bab0aa4151cd9854ac20f7cee856f8c"}');
CREATE TABLE render_job_heads (
        job_id TEXT PRIMARY KEY REFERENCES render_jobs(job_id),
        high_water INTEGER NOT NULL CHECK(high_water BETWEEN 1 AND 9223372036854775807),
        latest_attempt_id TEXT NOT NULL,
        FOREIGN KEY(job_id,latest_attempt_id) REFERENCES render_attempts(job_id,attempt_id)
    ) STRICT;
INSERT INTO "render_job_heads" VALUES('durable-structural',2,'structural-verify-2');
CREATE TABLE render_jobs (
        job_id TEXT PRIMARY KEY, revision_id TEXT NOT NULL REFERENCES revisions(id),
        intent TEXT NOT NULL CHECK(json_valid(intent))
    ) STRICT;
INSERT INTO "render_jobs" VALUES('durable-structural','background','{"schema_version":1,"job_id":"durable-structural","project_id":"project-picture-qualification","revision_id":"background","document_sha256":"af8e5672cee86b4d64827ef4e86b8af5ad9b9ca11d5c8b46e18e0f4d7d5663ff","range":{"start":20,"end":128},"policy":{"schema_version":1,"selection":"explicit_engineering","encoder":"hardware","b_frames":"none"}}');
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('initial',NULL,'initial','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "initial",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
    "frame_rate": {
      "numerator": 30,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "provisional",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "root",
  "nodes": {
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": []
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('registered','initial','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "registered",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "source"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('framed','registered','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "framed",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "source"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "3",
              "denominator": "5"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "5"
            },
            "scale": {
              "numerator": "7",
              "denominator": "5"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('fast','framed','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "fast",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "fast"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "3",
              "denominator": "5"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "5"
            },
            "scale": {
              "numerator": "7",
              "denominator": "5"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('repeat','fast','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "repeat",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "3",
              "denominator": "5"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "5"
            },
            "scale": {
              "numerator": "7",
              "denominator": "5"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('freeze','repeat','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "freeze",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "3",
              "denominator": "5"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "5"
            },
            "scale": {
              "numerator": "7",
              "denominator": "5"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('background','freeze','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "background",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "3",
              "denominator": "5"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "5"
            },
            "scale": {
              "numerator": "7",
              "denominator": "5"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('live-change','background','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "live-change",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
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
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('live-undo','live-change','undo','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "live-undo",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "3",
              "denominator": "5"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "5"
            },
            "scale": {
              "numerator": "7",
              "denominator": "5"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('live-redo','live-undo','redo','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "live-redo",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
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
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('worker-live-change','live-redo','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "worker-live-change",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "1",
              "denominator": "3"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "3"
            },
            "scale": {
              "numerator": "3",
              "denominator": "2"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('worker-live-undo','worker-live-change','undo','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "worker-live-undo",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
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
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('worker-live-redo','worker-live-undo','redo','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "worker-live-redo",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "1",
              "denominator": "3"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "3"
            },
            "scale": {
              "numerator": "3",
              "denominator": "2"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('encoded-live-change','worker-live-redo','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "encoded-live-change",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "1",
              "denominator": "4"
            },
            "center_y": {
              "numerator": "3",
              "denominator": "4"
            },
            "scale": {
              "numerator": "5",
              "denominator": "4"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('encoded-live-undo','encoded-live-change','undo','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "encoded-live-undo",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "1",
              "denominator": "3"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "3"
            },
            "scale": {
              "numerator": "3",
              "denominator": "2"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('encoded-live-redo','encoded-live-undo','redo','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "encoded-live-redo",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "1",
              "denominator": "4"
            },
            "center_y": {
              "numerator": "3",
              "denominator": "4"
            },
            "scale": {
              "numerator": "5",
              "denominator": "4"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('durable-live-edit','encoded-live-redo','edit','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "durable-live-edit",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "1",
              "denominator": "3"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "3"
            },
            "scale": {
              "numerator": "3",
              "denominator": "2"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('durable-live-undo','durable-live-edit','undo','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "durable-live-undo",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "1",
              "denominator": "4"
            },
            "center_y": {
              "numerator": "3",
              "denominator": "4"
            },
            "scale": {
              "numerator": "5",
              "denominator": "4"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('durable-live-redo','durable-live-undo','redo','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "durable-live-redo",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "1",
              "denominator": "3"
            },
            "center_y": {
              "numerator": "2",
              "denominator": "3"
            },
            "scale": {
              "numerator": "3",
              "denominator": "2"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('durable-live-restored','durable-live-redo','undo','{
  "schema_version": 33,
  "project_id": "project-picture-qualification",
  "revision_id": "durable-live-restored",
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
    "rate_origin": "primary_source",
    "geometry_origin": "primary_source",
    "primary": {
      "asset": "original",
      "qualification": "c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }
  },
  "root": "root",
  "nodes": {
    "background": {
      "label": "background",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 3,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "fast": {
      "label": "Retime",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 60,
        "mapping": {
          "start": 0,
          "end": 120
        },
        "pitch": "follow_speed"
      }
    },
    "freeze": {
      "label": "freeze",
      "kind": {
        "type": "hold",
        "recipe": {
          "picture_context": {
            "canvases": [
              {
                "width": 320,
                "height": 180,
                "fit": "fit",
                "layers": [
                  {
                    "center_x": {
                      "numerator": "3",
                      "denominator": "5"
                    },
                    "center_y": {
                      "numerator": "2",
                      "denominator": "5"
                    },
                    "scale": {
                      "numerator": "7",
                      "denominator": "5"
                    }
                  }
                ]
              }
            ]
          },
          "duration": 3,
          "video": {
            "type": "freeze",
            "asset": "original",
            "timestamp": {
              "ticks": 41041,
              "time_base": {
                "numerator": 1,
                "denominator": 30000
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "fast",
        "iterations": {
          "runs": [
            {
              "allocation": "repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": {
          "duration": 2,
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
          "repeat",
          "freeze",
          "background"
        ]
      }
    },
    "source": {
      "label": "Full Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 120,
          "video": {
            "type": "stream",
            "asset": "original",
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
            "asset": "original",
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
      },
      "framing": {
        "value": {
          "type": "static",
          "pose": {
            "center_x": {
              "numerator": "1",
              "denominator": "4"
            },
            "center_y": {
              "numerator": "3",
              "denominator": "4"
            },
            "scale": {
              "numerator": "5",
              "denominator": "4"
            }
          }
        }
      }
    }
  },
  "assets": {
    "original": {
      "label": "Synthetic original",
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
INSERT INTO "source_qualifications" VALUES('c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6','blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1','{"content":{"algorithm":"blake3","digest":"16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1"},"byte_length":39157}',X'7B22736368656D615F76657273696F6E223A312C226465636F6465725F636F6E7472616374223A2266666D7065672D382E302E332F736F757263652D6465636F6465642D7631222C2274696D696E675F706F6C6963795F76657273696F6E223A312C22636F6E74656E74223A7B22736861323536223A5B39302C3133302C31302C3132312C3139312C38352C31332C37322C37372C3134322C3230332C35352C3235352C3232312C3230382C37322C3230382C39392C3130332C3134382C3233352C3230362C39332C3138342C36322C37392C39322C3232392C3234352C3233302C37332C32345D2C22627974655F6C656E677468223A33393135377D2C226F726967696E5F7365636F6E6473223A7B226E756D657261746F72223A2230222C2264656E6F6D696E61746F72223A2231227D2C22766964656F223A7B22696E646578223A7B22736368656D615F76657273696F6E223A312C22636F6E74656E74223A7B22736861323536223A5B39302C3133302C31302C3132312C3139312C38352C31332C37322C37372C3134322C3230332C35352C3235352C3232312C3230382C37322C3230382C39392C3130332C3134382C3233352C3230362C39332C3138342C36322C37392C39322C3232392C3234352C3233302C37332C32345D2C22627974655F6C656E677468223A33393135377D2C2273747265616D5F696E646578223A302C22696E646578223A7B226173736574223A227175616C69666965642D736F75726365222C2274696D655F62617365223A7B226E756D657261746F72223A312C2264656E6F6D696E61746F72223A33303030307D2C226672616D6573223A5B7B226964656E74697479223A302C22707473223A302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A307D2C7B226964656E74697479223A312C22707473223A313030312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A313030317D2C7B226964656E74697479223A322C22707473223A323030322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A323030327D2C7B226964656E74697479223A332C22707473223A333030332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A333030337D2C7B226964656E74697479223A342C22707473223A343030342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A343030347D2C7B226964656E74697479223A352C22707473223A353030352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A353030357D2C7B226964656E74697479223A362C22707473223A363030362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A363030367D2C7B226964656E74697479223A372C22707473223A373030372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A373030377D2C7B226964656E74697479223A382C22707473223A383030382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A383030387D2C7B226964656E74697479223A392C22707473223A393030392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A393030397D2C7B226964656E74697479223A31302C22707473223A31303031302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31303031307D2C7B226964656E74697479223A31312C22707473223A31313031312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31313031317D2C7B226964656E74697479223A31322C22707473223A31323031322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31323031327D2C7B226964656E74697479223A31332C22707473223A31333031332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31333031337D2C7B226964656E74697479223A31342C22707473223A31343031342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A31343031347D2C7B226964656E74697479223A31352C22707473223A31353031352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31353031357D2C7B226964656E74697479223A31362C22707473223A31363031362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31363031367D2C7B226964656E74697479223A31372C22707473223A31373031372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31373031377D2C7B226964656E74697479223A31382C22707473223A31383031382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31383031387D2C7B226964656E74697479223A31392C22707473223A31393031392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A31393031397D2C7B226964656E74697479223A32302C22707473223A32303032302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32303032307D2C7B226964656E74697479223A32312C22707473223A32313032312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32313032317D2C7B226964656E74697479223A32322C22707473223A32323032322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32323032327D2C7B226964656E74697479223A32332C22707473223A32333032332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32333032337D2C7B226964656E74697479223A32342C22707473223A32343032342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32343032347D2C7B226964656E74697479223A32352C22707473223A32353032352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32353032357D2C7B226964656E74697479223A32362C22707473223A32363032362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32363032367D2C7B226964656E74697479223A32372C22707473223A32373032372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32373032377D2C7B226964656E74697479223A32382C22707473223A32383032382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32383032387D2C7B226964656E74697479223A32392C22707473223A32393032392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A32393032397D2C7B226964656E74697479223A33302C22707473223A33303033302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33303033307D2C7B226964656E74697479223A33312C22707473223A33313033312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33313033317D2C7B226964656E74697479223A33322C22707473223A33323033322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33323033327D2C7B226964656E74697479223A33332C22707473223A33333033332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33333033337D2C7B226964656E74697479223A33342C22707473223A33343033342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33343033347D2C7B226964656E74697479223A33352C22707473223A33353033352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33353033357D2C7B226964656E74697479223A33362C22707473223A33363033362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33363033367D2C7B226964656E74697479223A33372C22707473223A33373033372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33373033377D2C7B226964656E74697479223A33382C22707473223A33383033382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33383033387D2C7B226964656E74697479223A33392C22707473223A33393033392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A33393033397D2C7B226964656E74697479223A34302C22707473223A34303034302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34303034307D2C7B226964656E74697479223A34312C22707473223A34313034312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34313034317D2C7B226964656E74697479223A34322C22707473223A34323034322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34323034327D2C7B226964656E74697479223A34332C22707473223A34333034332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34333034337D2C7B226964656E74697479223A34342C22707473223A34343034342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A34343034347D2C7B226964656E74697479223A34352C22707473223A34353034352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34353034357D2C7B226964656E74697479223A34362C22707473223A34363034362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34363034367D2C7B226964656E74697479223A34372C22707473223A34373034372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34373034377D2C7B226964656E74697479223A34382C22707473223A34383034382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34383034387D2C7B226964656E74697479223A34392C22707473223A34393034392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A34393034397D2C7B226964656E74697479223A35302C22707473223A35303035302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35303035307D2C7B226964656E74697479223A35312C22707473223A35313035312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35313035317D2C7B226964656E74697479223A35322C22707473223A35323035322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35323035327D2C7B226964656E74697479223A35332C22707473223A35333035332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35333035337D2C7B226964656E74697479223A35342C22707473223A35343035342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35343035347D2C7B226964656E74697479223A35352C22707473223A35353035352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35353035357D2C7B226964656E74697479223A35362C22707473223A35363035362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35363035367D2C7B226964656E74697479223A35372C22707473223A35373035372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35373035377D2C7B226964656E74697479223A35382C22707473223A35383035382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35383035387D2C7B226964656E74697479223A35392C22707473223A35393035392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A35393035397D2C7B226964656E74697479223A36302C22707473223A36303036302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36303036307D2C7B226964656E74697479223A36312C22707473223A36313036312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36313036317D2C7B226964656E74697479223A36322C22707473223A36323036322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36323036327D2C7B226964656E74697479223A36332C22707473223A36333036332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36333036337D2C7B226964656E74697479223A36342C22707473223A36343036342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36343036347D2C7B226964656E74697479223A36352C22707473223A36353036352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36353036357D2C7B226964656E74697479223A36362C22707473223A36363036362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36363036367D2C7B226964656E74697479223A36372C22707473223A36373036372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36373036377D2C7B226964656E74697479223A36382C22707473223A36383036382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36383036387D2C7B226964656E74697479223A36392C22707473223A36393036392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A36393036397D2C7B226964656E74697479223A37302C22707473223A37303037302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37303037307D2C7B226964656E74697479223A37312C22707473223A37313037312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37313037317D2C7B226964656E74697479223A37322C22707473223A37323037322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37323037327D2C7B226964656E74697479223A37332C22707473223A37333037332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37333037337D2C7B226964656E74697479223A37342C22707473223A37343037342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A37343037347D2C7B226964656E74697479223A37352C22707473223A37353037352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37353037357D2C7B226964656E74697479223A37362C22707473223A37363037362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37363037367D2C7B226964656E74697479223A37372C22707473223A37373037372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37373037377D2C7B226964656E74697479223A37382C22707473223A37383037382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37383037387D2C7B226964656E74697479223A37392C22707473223A37393037392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A37393037397D2C7B226964656E74697479223A38302C22707473223A38303038302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38303038307D2C7B226964656E74697479223A38312C22707473223A38313038312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38313038317D2C7B226964656E74697479223A38322C22707473223A38323038322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38323038327D2C7B226964656E74697479223A38332C22707473223A38333038332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38333038337D2C7B226964656E74697479223A38342C22707473223A38343038342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38343038347D2C7B226964656E74697479223A38352C22707473223A38353038352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38353038357D2C7B226964656E74697479223A38362C22707473223A38363038362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38363038367D2C7B226964656E74697479223A38372C22707473223A38373038372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38373038377D2C7B226964656E74697479223A38382C22707473223A38383038382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38383038387D2C7B226964656E74697479223A38392C22707473223A38393038392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A38393038397D2C7B226964656E74697479223A39302C22707473223A39303039302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39303039307D2C7B226964656E74697479223A39312C22707473223A39313039312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39313039317D2C7B226964656E74697479223A39322C22707473223A39323039322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39323039327D2C7B226964656E74697479223A39332C22707473223A39333039332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39333039337D2C7B226964656E74697479223A39342C22707473223A39343039342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39343039347D2C7B226964656E74697479223A39352C22707473223A39353039352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39353039357D2C7B226964656E74697479223A39362C22707473223A39363039362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39363039367D2C7B226964656E74697479223A39372C22707473223A39373039372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39373039377D2C7B226964656E74697479223A39382C22707473223A39383039382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39383039387D2C7B226964656E74697479223A39392C22707473223A39393039392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A39393039397D2C7B226964656E74697479223A3130302C22707473223A3130303130302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130303130307D2C7B226964656E74697479223A3130312C22707473223A3130313130312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130313130317D2C7B226964656E74697479223A3130322C22707473223A3130323130322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130323130327D2C7B226964656E74697479223A3130332C22707473223A3130333130332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130333130337D2C7B226964656E74697479223A3130342C22707473223A3130343130342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3130343130347D2C7B226964656E74697479223A3130352C22707473223A3130353130352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130353130357D2C7B226964656E74697479223A3130362C22707473223A3130363130362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130363130367D2C7B226964656E74697479223A3130372C22707473223A3130373130372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130373130377D2C7B226964656E74697479223A3130382C22707473223A3130383130382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130383130387D2C7B226964656E74697479223A3130392C22707473223A3130393130392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3130393130397D2C7B226964656E74697479223A3131302C22707473223A3131303131302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131303131307D2C7B226964656E74697479223A3131312C22707473223A3131313131312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131313131317D2C7B226964656E74697479223A3131322C22707473223A3131323131322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131323131327D2C7B226964656E74697479223A3131332C22707473223A3131333131332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131333131337D2C7B226964656E74697479223A3131342C22707473223A3131343131342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131343131347D2C7B226964656E74697479223A3131352C22707473223A3131353131352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131353131357D2C7B226964656E74697479223A3131362C22707473223A3131363131362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131363131367D2C7B226964656E74697479223A3131372C22707473223A3131373131372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131373131377D2C7B226964656E74697479223A3131382C22707473223A3131383131382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3131383131387D2C7B226964656E74697479223A3131392C22707473223A3131393131392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A6E756C6C7D5D2C227465726D696E616C5F656E64223A3132303132302C227465726D696E616C5F70726F76656E616E6365223A226465636F6465645F6672616D655F6475726174696F6E227D7D2C22696E746572707265746174696F6E223A7B227769647468223A3332302C22686569676874223A3138302C2273747265616D5F696E646578223A302C2274696D655F626173655F6E756D223A312C2274696D655F626173655F64656E223A33303030302C2273616D706C655F6173706563745F6E756D223A312C2273616D706C655F6173706563745F64656E223A312C22726F746174696F6E5F717561727465725F7475726E73223A302C22636F6C6F72223A7B2272616E6765223A226C696D69746564222C226D6174726978223A226274373039222C227472616E73666572223A226274373039222C227072696D6172696573223A226274373039227D2C22636F646563223A2268323634222C22706978656C5F666F726D6174223A2279757634323070222C2273747265616D5F7374617274223A302C2273747265616D5F6475726174696F6E223A3132303132302C22636F6E7461696E65725F7374617274223A6E756C6C2C22636F6E7461696E65725F6475726174696F6E223A6E756C6C2C22617564696F5F73747265616D73223A5B7B2273747265616D5F696E646578223A312C22636F646563223A22616163222C2274696D655F626173655F6E756D223A312C2274696D655F626173655F64656E223A34383030302C2273747265616D5F7374617274223A302C2273747265616D5F6475726174696F6E223A3139323139322C2273616D706C655F72617465223A34383030302C226368616E6E656C5F636F756E74223A327D5D7D7D2C22617564696F223A7B22736368656D615F76657273696F6E223A312C226465636F6465725F636F6E7472616374223A2266666D7065672D382E302E332F617564696F2D6D616E75616C2D736B69702D7631222C22636F6E74656E74223A7B22736861323536223A5B39302C3133302C31302C3132312C3139312C38352C31332C37322C37372C3134322C3230332C35352C3235352C3232312C3230382C37322C3230382C39392C3130332C3134382C3233352C3230362C39332C3138342C36322C37392C39322C3232392C3234352C3233302C37332C32345D2C22627974655F6C656E677468223A33393135377D2C2273747265616D223A7B2273747265616D5F696E646578223A312C22636F646563223A22616163222C2274696D655F62617365223A7B226E756D657261746F72223A312C2264656E6F6D696E61746F72223A34383030307D2C2273616D706C655F72617465223A34383030302C226368616E6E656C5F6C61796F7574223A7B226F72646572223A226E6174697665222C226368616E6E656C73223A322C226D61736B223A337D2C2273747265616D5F7374617274223A302C2273747265616D5F6475726174696F6E223A3139323139322C22696E697469616C5F70616464696E67223A302C22747261696C696E675F70616464696E67223A302C227365656B5F707265726F6C6C223A307D2C226F62736572766174696F6E73223A5B7B22707473223A2D313032342C2264697363617264223A747275652C226465636F64655F74696D657374616D70223A2D313032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A7B226C656164696E67223A313032342C22747261696C696E67223A302C226C656164696E675F726561736F6E223A302C22747261696C696E675F726561736F6E223A307D7D2C7B22707473223A302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A313032342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A313032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A323034382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A323034382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A333037322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A333037322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A343039362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A343039362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A353132302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A353132302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A363134342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A363134342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A373136382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A373136382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A383139322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A383139322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A393231362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A393231362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31303234302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31303234302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31313236342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31313236342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31323238382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31323238382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31333331322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31333331322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31343333362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31343333362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31353336302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31353336302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31363338342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31363338342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31373430382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31373430382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31383433322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31383433322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A31393435362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A31393435362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32303438302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32303438302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32313530342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32313530342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32323532382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32323532382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32333535322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32333535322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32343537362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32343537362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32353630302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32353630302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32363632342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32363632342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32373634382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32373634382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32383637322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32383637322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A32393639362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A32393639362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33303732302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33303732302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33313734342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33313734342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33323736382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33323736382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33333739322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33333739322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33343831362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33343831362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33353834302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33353834302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33363836342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33363836342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33373838382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33373838382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33383931322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33383931322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A33393933362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A33393933362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34303936302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34303936302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34313938342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34313938342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34333030382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34333030382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34343033322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34343033322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34353035362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34353035362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34363038302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34363038302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34373130342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34373130342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34383132382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34383132382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A34393135322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A34393135322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35303137362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35303137362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35313230302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35313230302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35323232342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35323232342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35333234382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35333234382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35343237322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35343237322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35353239362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35353239362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35363332302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35363332302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35373334342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35373334342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35383336382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35383336382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A35393339322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A35393339322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36303431362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36303431362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36313434302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36313434302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36323436342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36323436342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36333438382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36333438382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36343531322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36343531322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36353533362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36353533362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36363536302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36363536302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36373538342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36373538342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36383630382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36383630382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A36393633322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A36393633322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37303635362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37303635362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37313638302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37313638302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37323730342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37323730342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37333732382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37333732382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37343735322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37343735322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37353737362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37353737362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37363830302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37363830302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37373832342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37373832342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37383834382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37383834382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A37393837322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A37393837322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38303839362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38303839362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38313932302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38313932302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38323934342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38323934342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38333936382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38333936382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38343939322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38343939322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38363031362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38363031362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38373034302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38373034302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38383036342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38383036342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A38393038382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A38393038382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39303131322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39303131322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39313133362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39313133362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39323136302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39323136302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39333138342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39333138342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39343230382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39343230382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39353233322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39353233322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39363235362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39363235362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39373238302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39373238302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39383330342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39383330342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39393332382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39393332382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130303335322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130303335322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130313337362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130313337362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130323430302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130323430302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130333432342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130333432342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130343434382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130343434382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130353437322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130353437322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130363439362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130363439362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130373532302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130373532302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130383534342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130383534342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130393536382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130393536382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131303539322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131303539322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131313631362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131313631362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131323634302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131323634302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131333636342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131333636342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131343638382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131343638382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131353731322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131353731322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131363733362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131363733362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131373736302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131373736302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131383738342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131383738342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131393830382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131393830382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132303833322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132303833322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132313835362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132313835362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132323838302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132323838302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132333930342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132333930342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132343932382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132343932382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132353935322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132353935322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132363937362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132363937362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132383030302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132383030302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132393032342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132393032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133303034382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133303034382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133313037322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133313037322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133323039362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133323039362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133333132302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133333132302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133343134342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133343134342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133353136382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133353136382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133363139322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133363139322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133373231362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133373231362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133383234302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133383234302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133393236342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133393236342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134303238382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134303238382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134313331322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134313331322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134323333362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134323333362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134333336302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134333336302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134343338342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134343338342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134353430382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134353430382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134363433322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134363433322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134373435362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134373435362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134383438302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134383438302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134393530342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134393530342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135303532382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135303532382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135313535322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135313535322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135323537362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135323537362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135333630302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135333630302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135343632342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135343632342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135353634382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135353634382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135363637322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135363637322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135373639362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135373639362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135383732302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135383732302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135393734342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135393734342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136303736382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136303736382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136313739322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136313739322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136323831362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136323831362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136333834302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136333834302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136343836342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136343836342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136353838382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136353838382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136363931322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136363931322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136373933362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136373933362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136383936302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136383936302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136393938342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136393938342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137313030382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137313030382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137323033322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137323033322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137333035362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137333035362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137343038302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137343038302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137353130342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137353130342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137363132382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137363132382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137373135322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137373135322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137383137362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137383137362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137393230302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137393230302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138303232342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138303232342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138313234382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138313234382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138323237322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138323237322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138333239362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138333239362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138343332302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138343332302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138353334342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138353334342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138363336382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138363336382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138373339322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138373339322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138383431362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138383431362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138393434302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138393434302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139303436342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139303436342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139313438382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139313438382C227265706F727465645F6475726174696F6E223A3730342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D5D7D7D');
CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id),
            workflow TEXT NOT NULL DEFAULT 'generic' CHECK(workflow IN ('generic','single_source_v1'))
        ) STRICT;
INSERT INTO "state" VALUES(1,'durable-live-restored',9,'generic');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
CREATE UNIQUE INDEX one_active_render_attempt ON render_attempts((1)) WHERE state IN ('queued','encoding','encoded_retained','verifying','cancelling');
CREATE UNIQUE INDEX unique_render_cancellation_token ON render_attempts(cancellation_token);
COMMIT;
PRAGMA application_id = 1146113585;
PRAGMA user_version = 40;
