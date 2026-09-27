PRAGMA application_id=1146113585;
PRAGMA user_version=28;
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
INSERT INTO "history" VALUES(1,NULL,'binding-gap-growth','{"project_id":"d038df46-94c6-4c0a-9d11-038b0e6da220","expected_revision":"859077f6-c39b-4091-8382-963caf57bfa9","new_revision":"binding-gap-growth","command":{"command":"set_repeat","node":"old-gap-owner","plays":4,"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}}','{"forward":{"project_id":"d038df46-94c6-4c0a-9d11-038b0e6da220","from_revision":"859077f6-c39b-4091-8382-963caf57bfa9","to_revision":"binding-gap-growth","nodes":{"old-gap-owner":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1},{"allocation":"old-gap-growth","first":0,"count":2}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1},{"allocation":"old-gap-growth","first":0,"count":2},{"allocation":"binding-gap-growth","first":0,"count":1}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"d038df46-94c6-4c0a-9d11-038b0e6da220","from_revision":"binding-gap-growth","to_revision":"859077f6-c39b-4091-8382-963caf57bfa9","nodes":{"old-gap-owner":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1},{"allocation":"old-gap-growth","first":0,"count":2},{"allocation":"binding-gap-growth","first":0,"count":1}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1},{"allocation":"old-gap-growth","first":0,"count":2}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["old-gap-owner"],"duration_delta":5,"description":"Set repeat parameters"}');
INSERT INTO "history" VALUES(2,1,'old-gap-rename','{"project_id":"d038df46-94c6-4c0a-9d11-038b0e6da220","expected_revision":"binding-gap-growth","new_revision":"old-gap-rename","command":{"command":"rename","node":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","label":"Retained core 22 gap binding"}}','{"forward":{"project_id":"d038df46-94c6-4c0a-9d11-038b0e6da220","from_revision":"binding-gap-growth","to_revision":"old-gap-rename","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"after":{"label":"Retained core 22 gap binding","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"d038df46-94c6-4c0a-9d11-038b0e6da220","from_revision":"old-gap-rename","to_revision":"binding-gap-growth","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Retained core 22 gap binding","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["b5903f58-0249-4a89-8af8-ed3d8a5cfe1f"],"duration_delta":0,"description":"Rename beat"}');
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
CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
INSERT INTO "redo" VALUES(1,2);
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('859077f6-c39b-4091-8382-963caf57bfa9',NULL,'initial','{"assets":{"audio":{"audio":{"end":{"ticks":48000,"time_base":{"denominator":48000,"numerator":1}},"start":{"ticks":0,"time_base":{"denominator":48000,"numerator":1}}},"content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","frame_count":null,"label":"Retained original audio fixture","still_image":false,"video":null}},"audio_bindings":{"bindings":{"hold":{"lattice":{"arguments":[],"births":[],"reference":{"physical":"hold","root":{"type":"project_root_round_even"},"timing":{"allocation":"splice","ordinal":0}}},"reanchors":[{"placement":{"arguments":[],"births":[],"reference":{"physical":"hold","root":{"type":"project_root_round_even"},"timing":{"allocation":"splice","ordinal":0}}}}],"resume":null},"reanchor-copy":{"lattice":{"arguments":[],"births":[],"reference":{"physical":"reanchor-copy","root":{"type":"project_root_round_even"},"timing":{"allocation":"append-reanchor","ordinal":0}}},"resume":{"local_boundary":{"denominator":"1","numerator":"1"},"phase":{"constant":{"denominator":"1","numerator":"0"},"terms":[{"from_local":{"denominator":"1","numerator":"0"},"placement":{"arguments":[],"births":[],"reference":{"physical":"reanchor-copy","root":{"type":"project_root_round_even"},"timing":{"allocation":"append-reanchor","ordinal":0}}},"to_local":{"denominator":"1","numerator":"1"}}]}}},"reanchor-pause":{"lattice":{"arguments":[{"reference_repeat":"old-gap-owner","value":{"repeat":"old-gap-owner","type":"live"}}],"births":[{"definition_root":"reanchor-pause","repeat":"old-gap-owner","survivors":{"repeat":"old-gap-owner","type":"captured_repeat"}}],"reference":{"physical":"reanchor-pause","root":{"type":"project_root_round_even"},"timing":{"allocation":"schema28-gap-capture","ordinal":0}}},"resume":null},"reanchor-split-1":{"lattice":{"arguments":[],"births":[],"reference":{"physical":"hold","root":{"type":"project_root_round_even"},"timing":{"allocation":"splice","ordinal":0}}},"reanchors":[{"placement":{"arguments":[],"births":[],"reference":{"physical":"hold","root":{"type":"project_root_round_even"},"timing":{"allocation":"splice","ordinal":0}}}},{"placement":{"arguments":[],"births":[],"reference":{"physical":"hold","root":{"type":"project_root_round_even"},"timing":{"allocation":"append-reanchor","ordinal":0}}},"window":{"end":{"denominator":"1","numerator":"40"},"start":{"denominator":"1","numerator":"1"}}}],"resume":null},"selected-pause-hold":{"lattice":{"arguments":[],"births":[],"reference":{"physical":"selected-pause-hold","root":{"type":"project_root_round_even"},"timing":{"allocation":"append-reanchor","ordinal":0}}},"resume":{"local_boundary":{"denominator":"1","numerator":"0"},"phase":{"constant":{"denominator":"1","numerator":"0"},"terms":[]}}},"selected-split-2":{"lattice":{"arguments":[],"births":[],"reference":{"physical":"source","root":{"type":"project_root_round_even"},"timing":{"allocation":"selected-pause","ordinal":0}}},"reanchors":[{"placement":{"arguments":[],"births":[],"reference":{"physical":"source","root":{"type":"project_root_round_even"},"timing":{"allocation":"selected-pause","ordinal":0}}}},{"placement":{"arguments":[],"births":[],"reference":{"physical":"selected-split-2","root":{"type":"project_root_round_even"},"timing":{"allocation":"append-reanchor","ordinal":0}}},"window":{"end":{"denominator":"1","numerator":"40"},"start":{"denominator":"1","numerator":"1"}}}],"resume":{"local_boundary":{"denominator":"1","numerator":"5"},"phase":{"constant":{"denominator":"1","numerator":"0"},"terms":[{"from_local":{"denominator":"1","numerator":"0"},"placement":{"arguments":[],"births":[],"reference":{"physical":"source","root":{"type":"project_root_round_even"},"timing":{"allocation":"selected-pause","ordinal":0}}},"to_local":{"denominator":"1","numerator":"5"}}]}}},"source":{"lattice":{"arguments":[],"births":[],"reference":{"physical":"source","root":{"type":"project_root_round_even"},"timing":{"allocation":"selected-pause","ordinal":0}}},"reanchors":[{"placement":{"arguments":[],"births":[],"reference":{"physical":"source","root":{"type":"project_root_round_even"},"timing":{"allocation":"selected-pause","ordinal":0}}}},{"placement":{"arguments":[],"births":[],"reference":{"physical":"source","root":{"type":"project_root_round_even"},"timing":{"allocation":"append-reanchor","ordinal":0}}},"window":{"end":{"denominator":"1","numerator":"40"},"start":{"denominator":"1","numerator":"1"}}}],"resume":null},"splice-copy-2":{"lattice":{"arguments":[],"births":[],"reference":{"physical":"hold","root":{"type":"project_root_round_even"},"timing":{"allocation":"splice","ordinal":0}}},"reanchors":[{"placement":{"arguments":[],"births":[],"reference":{"physical":"hold","root":{"type":"project_root_round_even"},"timing":{"allocation":"splice","ordinal":0}}}},{"placement":{"arguments":[],"births":[],"reference":{"physical":"splice-copy-2","root":{"type":"project_root_round_even"},"timing":{"allocation":"append-reanchor","ordinal":0}}},"window":{"end":{"denominator":"1","numerator":"40"},"start":{"denominator":"1","numerator":"1"}}}],"resume":{"local_boundary":{"denominator":"1","numerator":"3"},"phase":{"constant":{"denominator":"1","numerator":"0"},"terms":[{"from_local":{"denominator":"1","numerator":"0"},"placement":{"arguments":[],"births":[],"reference":{"physical":"hold","root":{"type":"project_root_round_even"},"timing":{"allocation":"splice","ordinal":0}}},"to_local":{"denominator":"1","numerator":"3"}}]}}}},"gap_bindings":{"old-gap-owner":{"lattice":{"arguments":[],"births":[],"gap_after":{"repeat":"old-gap-owner","type":"live"},"reference":{"physical":"old-gap-owner","recipe":"repeat_gap","root":{"type":"project_root_round_even"},"timing":{"allocation":"schema28-gap-capture","ordinal":0}}},"resume":null}},"timings":[{"id":{"allocation":"append-reanchor","ordinal":0},"layout":{"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"reanchor-copy":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"selected-pause-hold":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"selected-split-2":{"allocation":"selected-pause","origin":"source"},"source":{"allocation":"selected-pause","origin":"source"},"splice-copy-2":{"allocation":"splice","origin":"hold"}},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":40,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"children":["splice-copy-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"],"type":"sequence"}},"hold":{"duration":8,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}},"reanchor-copy":{"duration":2,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}},"reanchor-left":{"duration":1,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"selected-pause-hold","mapping":{"end":1,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"}},"reanchor-right":{"duration":1,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"reanchor-copy","mapping":{"end":2,"start":1},"pitch":"preserve","purpose":"partition","type":"retime"}},"selected-pause-hold":{"duration":2,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}},"selected-split-0":{"duration":5,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"source","mapping":{"end":5,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"}},"selected-split-1":{"duration":25,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"selected-split-2","mapping":{"end":30,"start":5},"pitch":"preserve","purpose":"partition","type":"retime"}},"selected-split-2":{"duration":30,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"placement":{"end":{"denominator":"8008","numerator":"160005"},"start":{"denominator":"8008","numerator":"15861"}},"type":"source"}},"source":{"duration":30,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"placement":{"end":{"denominator":"8008","numerator":"160005"},"start":{"denominator":"8008","numerator":"15861"}},"type":"source"}},"splice-copy-0":{"duration":3,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"hold","mapping":{"end":3,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"}},"splice-copy-1":{"duration":5,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"splice-copy-2","mapping":{"end":8,"start":3},"pitch":"preserve","purpose":"partition","type":"retime"}},"splice-copy-2":{"duration":8,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}}},"overrides":{},"rate":{"denominator":1001,"numerator":30000},"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f"}},{"id":{"allocation":"schema28-gap-capture","ordinal":0},"layout":{"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"reanchor-copy":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"reanchor-split-1":{"allocation":"splice","origin":"hold"},"selected-pause-hold":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"selected-split-2":{"allocation":"selected-pause","origin":"source"},"source":{"allocation":"selected-pause","origin":"source"},"splice-copy-2":{"allocation":"splice","origin":"hold"}},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":52,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"],"type":"sequence"}},"hold":{"duration":8,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}},"old-gap-owner":{"duration":12,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"reanchor-pause","gap_audio":{"type":"silence"},"gap_duration":3,"iterations":{"runs":[{"allocation":"old-configured-gap","count":1,"first":0},{"allocation":"old-gap-growth","count":2,"first":0}]},"type":"repeat"}},"reanchor-copy":{"duration":2,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}},"reanchor-left":{"duration":1,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"selected-pause-hold","mapping":{"end":1,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"}},"reanchor-pause":{"duration":2,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}},"reanchor-right":{"duration":1,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"reanchor-copy","mapping":{"end":2,"start":1},"pitch":"preserve","purpose":"partition","type":"retime"}},"reanchor-split-0":{"duration":2,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"reanchor-split-1","mapping":{"end":3,"start":1},"pitch":"preserve","purpose":"partition","type":"retime"}},"reanchor-split-1":{"duration":8,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}},"selected-pause-hold":{"duration":2,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}},"selected-split-0":{"duration":5,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"source","mapping":{"end":5,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"}},"selected-split-1":{"duration":25,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"selected-split-2","mapping":{"end":30,"start":5},"pitch":"preserve","purpose":"partition","type":"retime"}},"selected-split-2":{"duration":30,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"placement":{"end":{"denominator":"8008","numerator":"160005"},"start":{"denominator":"8008","numerator":"15861"}},"type":"source"}},"source":{"duration":30,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"placement":{"end":{"denominator":"8008","numerator":"160005"},"start":{"denominator":"8008","numerator":"15861"}},"type":"source"}},"splice-copy-0":{"duration":1,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"hold","mapping":{"end":1,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"}},"splice-copy-1":{"duration":5,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"splice-copy-2","mapping":{"end":8,"start":3},"pitch":"preserve","purpose":"partition","type":"retime"}},"splice-copy-2":{"duration":8,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}}},"overrides":{},"rate":{"denominator":1001,"numerator":30000},"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f"}},{"id":{"allocation":"selected-pause","ordinal":0},"layout":{"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"splice-copy-2":{"allocation":"splice","origin":"hold"}},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":38,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"children":["splice-copy-0","splice-copy-1","source"],"type":"sequence"}},"hold":{"duration":8,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}},"source":{"duration":30,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"placement":{"end":{"denominator":"8008","numerator":"160005"},"start":{"denominator":"8008","numerator":"15861"}},"type":"source"}},"splice-copy-0":{"duration":3,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"hold","mapping":{"end":3,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"}},"splice-copy-1":{"duration":5,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"child":"splice-copy-2","mapping":{"end":8,"start":3},"pitch":"preserve","purpose":"partition","type":"retime"}},"splice-copy-2":{"duration":8,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}}},"overrides":{},"rate":{"denominator":1001,"numerator":30000},"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f"}},{"id":{"allocation":"splice","ordinal":0},"layout":{"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":8,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"children":["hold"],"type":"sequence"}},"hold":{"duration":8,"edges":{"node_end":"automatic","node_start":"automatic","repeat_gap_end":"automatic","repeat_gap_start":"automatic","source_placement_end":"automatic","source_placement_start":"automatic"},"kind":{"audio":{"type":"silence"},"type":"hold"}}},"overrides":{},"rate":{"denominator":1001,"numerator":30000},"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f"}}]},"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"reanchor-copy":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"reanchor-split-1":{"allocation":"splice","origin":"hold"},"selected-pause-hold":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"selected-split-2":{"allocation":"selected-pause","origin":"source"},"source":{"allocation":"selected-pause","origin":"source"},"splice-copy-2":{"allocation":"splice","origin":"hold"}},"basis_state":{"geometry_origin":"explicit","primary":null,"rate_origin":"explicit"},"marks":{},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"kind":{"children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"],"type":"sequence"},"label":"Sequence"},"hold":{"framing":{"value":{"envelope":{"initial":{"center_x":{"denominator":"2","numerator":"1"},"center_y":{"denominator":"2","numerator":"1"},"scale":{"denominator":"1","numerator":"1"}},"segments":[{"curve":{"type":"smoothstep"},"end":{"denominator":"1","numerator":"1"},"pose":{"center_x":{"denominator":"2","numerator":"1"},"center_y":{"denominator":"2","numerator":"1"},"scale":{"denominator":"20","numerator":"27"}}}]},"type":"envelope"}},"kind":{"recipe":{"audio":{"type":"silence"},"duration":8,"video":{"type":"background"}},"type":"hold"},"label":"Pause"},"old-gap-owner":{"kind":{"child":"reanchor-pause","gap":{"audio":{"type":"silence"},"duration":3,"video":{"type":"background"}},"iterations":{"runs":[{"allocation":"old-configured-gap","count":1,"first":0},{"allocation":"old-gap-growth","count":2,"first":0}]},"type":"repeat"},"label":"Repeat"},"reanchor-copy":{"kind":{"recipe":{"audio":{"type":"silence"},"duration":2,"video":{"type":"background"}},"type":"hold"},"label":"Pause"},"reanchor-left":{"kind":{"child":"selected-pause-hold","duration":1,"mapping":{"end":1,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"},"label":"Pause"},"reanchor-pause":{"kind":{"recipe":{"audio":{"type":"silence"},"duration":2,"video":{"type":"background"}},"type":"hold"},"label":"Pause"},"reanchor-right":{"kind":{"child":"reanchor-copy","duration":1,"mapping":{"end":2,"start":1},"pitch":"preserve","purpose":"partition","type":"retime"},"label":"Pause"},"reanchor-split-0":{"kind":{"child":"reanchor-split-1","duration":2,"mapping":{"end":3,"start":1},"pitch":"preserve","purpose":"partition","type":"retime"},"label":"Pause"},"reanchor-split-1":{"framing":{"value":{"envelope":{"initial":{"center_x":{"denominator":"2","numerator":"1"},"center_y":{"denominator":"2","numerator":"1"},"scale":{"denominator":"1","numerator":"1"}},"segments":[{"curve":{"type":"smoothstep"},"end":{"denominator":"1","numerator":"1"},"pose":{"center_x":{"denominator":"2","numerator":"1"},"center_y":{"denominator":"2","numerator":"1"},"scale":{"denominator":"20","numerator":"27"}}}]},"type":"envelope"}},"kind":{"recipe":{"audio":{"type":"silence"},"duration":8,"video":{"type":"background"}},"type":"hold"},"label":"Pause"},"selected-pause-hold":{"kind":{"recipe":{"audio":{"type":"silence"},"duration":2,"video":{"type":"background"}},"type":"hold"},"label":"Pause"},"selected-split-0":{"kind":{"child":"source","duration":5,"mapping":{"end":5,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"},"label":"Retained pending rename"},"selected-split-1":{"kind":{"child":"selected-split-2","duration":25,"mapping":{"end":30,"start":5},"pitch":"preserve","purpose":"partition","type":"retime"},"label":"Retained pending rename"},"selected-split-2":{"kind":{"source":{"audio":{"asset":"audio","span":{"end":{"ticks":48000,"time_base":{"denominator":48000,"numerator":1}},"start":{"ticks":0,"time_base":{"denominator":48000,"numerator":1}}}},"audio_mapping":{"frames":{"denominator":"1001","numerator":"30000"},"selection":{"end":{"denominator":"1","numerator":"20"},"start":{"denominator":"1","numerator":"2"}},"start":{"denominator":"3","numerator":"-2"},"type":"selected_placement"},"audio_offset":-31,"duration":30,"link":"independent","video":{"type":"blank"},"video_mapping":{"type":"fit_beat"}},"type":"source"},"label":"Retained pending rename"},"source":{"kind":{"source":{"audio":{"asset":"audio","span":{"end":{"ticks":48000,"time_base":{"denominator":48000,"numerator":1}},"start":{"ticks":0,"time_base":{"denominator":48000,"numerator":1}}}},"audio_mapping":{"frames":{"denominator":"1001","numerator":"30000"},"selection":{"end":{"denominator":"1","numerator":"20"},"start":{"denominator":"1","numerator":"2"}},"start":{"denominator":"3","numerator":"-2"},"type":"selected_placement"},"audio_offset":-31,"duration":30,"link":"independent","video":{"type":"blank"},"video_mapping":{"type":"fit_beat"}},"type":"source"},"label":"Retained pending rename"},"splice-copy-0":{"kind":{"child":"hold","duration":1,"mapping":{"end":1,"start":0},"pitch":"preserve","purpose":"partition","type":"retime"},"label":"Pause"},"splice-copy-1":{"kind":{"child":"splice-copy-2","duration":5,"mapping":{"end":8,"start":3},"pitch":"preserve","purpose":"partition","type":"retime"},"label":"Pause"},"splice-copy-2":{"framing":{"value":{"envelope":{"initial":{"center_x":{"denominator":"2","numerator":"1"},"center_y":{"denominator":"2","numerator":"1"},"scale":{"denominator":"1","numerator":"1"}},"segments":[{"curve":{"type":"smoothstep"},"end":{"denominator":"1","numerator":"1"},"pose":{"center_x":{"denominator":"2","numerator":"1"},"center_y":{"denominator":"2","numerator":"1"},"scale":{"denominator":"20","numerator":"27"}}}]},"type":"envelope"}},"kind":{"recipe":{"audio":{"type":"silence"},"duration":8,"video":{"type":"background"}},"type":"hold"},"label":"Pause"}},"overrides":{},"presentation_basis":{"color_policy":"sdr_rec709","frame_rate":{"denominator":1001,"numerator":30000},"height":16,"width":16},"project_id":"d038df46-94c6-4c0a-9d11-038b0e6da220","revision_id":"859077f6-c39b-4091-8382-963caf57bfa9","root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","schema_version":22}');
INSERT INTO "revisions" VALUES('binding-gap-growth','859077f6-c39b-4091-8382-963caf57bfa9','edit','{
  "schema_version": 22,
  "project_id": "d038df46-94c6-4c0a-9d11-038b0e6da220",
  "revision_id": "binding-gap-growth",
  "presentation_basis": {
    "width": 16,
    "height": 16,
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
  "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
  "nodes": {
    "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "splice-copy-0",
          "old-gap-owner",
          "reanchor-split-0",
          "splice-copy-1",
          "selected-split-0",
          "reanchor-left",
          "reanchor-right",
          "selected-split-1"
        ]
      }
    },
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "old-gap-owner": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "reanchor-pause",
        "iterations": {
          "runs": [
            {
              "allocation": "old-configured-gap",
              "first": 0,
              "count": 1
            },
            {
              "allocation": "old-gap-growth",
              "first": 0,
              "count": 2
            },
            {
              "allocation": "binding-gap-growth",
              "first": 0,
              "count": 1
            }
          ]
        },
        "gap": {
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
    "reanchor-copy": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-left": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "selected-pause-hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-pause": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-right": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-copy",
        "duration": 1,
        "mapping": {
          "start": 1,
          "end": 2
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-split-1",
        "duration": 2,
        "mapping": {
          "start": 1,
          "end": 3
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-1": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "selected-pause-hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "selected-split-0": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-1": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "selected-split-2",
        "duration": 25,
        "mapping": {
          "start": 5,
          "end": 30
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-2": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "source": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "splice-copy-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "splice-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-2": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    }
  },
  "assets": {
    "audio": {
      "label": "Retained original audio fixture",
      "content_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "video": null,
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": null
    }
  },
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "splice",
      "origin": "hold"
    },
    "reanchor-copy": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "reanchor-split-1": {
      "allocation": "splice",
      "origin": "hold"
    },
    "selected-pause-hold": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "selected-split-2": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "source": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "splice-copy-2": {
      "allocation": "splice",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "append-reanchor",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 40,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "schema28-gap-capture",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 52,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "old-gap-owner",
                  "reanchor-split-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "old-gap-owner": {
              "duration": 12,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "repeat",
                "child": "reanchor-pause",
                "iterations": {
                  "runs": [
                    {
                      "allocation": "old-configured-gap",
                      "first": 0,
                      "count": 1
                    },
                    {
                      "allocation": "old-gap-growth",
                      "first": 0,
                      "count": 2
                    }
                  ]
                },
                "gap_duration": 3,
                "gap_audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-pause": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-0": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-split-1",
                "mapping": {
                  "start": 1,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-1": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "reanchor-split-1": {
              "allocation": "splice",
              "origin": "hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "selected-pause",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 38,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "source"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "splice",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "hold"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {}
        }
      }
    ],
    "bindings": {
      "hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          }
        ]
      },
      "reanchor-copy": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-copy"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "1",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "append-reanchor",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "reanchor-copy"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "1",
                  "denominator": "1"
                }
              }
            ]
          }
        }
      },
      "reanchor-pause": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-pause"
          },
          "arguments": [
            {
              "reference_repeat": "old-gap-owner",
              "value": {
                "type": "live",
                "repeat": "old-gap-owner"
              }
            }
          ],
          "births": [
            {
              "repeat": "old-gap-owner",
              "survivors": {
                "type": "captured_repeat",
                "repeat": "old-gap-owner"
              },
              "definition_root": "reanchor-pause"
            }
          ]
        },
        "resume": null
      },
      "reanchor-split-1": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "selected-pause-hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "selected-pause-hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "0",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": []
          }
        }
      },
      "selected-split-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "5",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "selected-pause",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "source"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "5",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "selected-split-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "source": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "splice-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "3",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "splice",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "hold"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "3",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "splice-copy-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      }
    },
    "gap_bindings": {
      "old-gap-owner": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "old-gap-owner",
            "recipe": "repeat_gap"
          },
          "gap_after": {
            "type": "live",
            "repeat": "old-gap-owner"
          },
          "arguments": [],
          "births": []
        },
        "resume": null
      }
    }
  }
}
');
INSERT INTO "revisions" VALUES('old-gap-rename','binding-gap-growth','edit','{
  "schema_version": 22,
  "project_id": "d038df46-94c6-4c0a-9d11-038b0e6da220",
  "revision_id": "old-gap-rename",
  "presentation_basis": {
    "width": 16,
    "height": 16,
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
  "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
  "nodes": {
    "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
      "label": "Retained core 22 gap binding",
      "kind": {
        "type": "sequence",
        "children": [
          "splice-copy-0",
          "old-gap-owner",
          "reanchor-split-0",
          "splice-copy-1",
          "selected-split-0",
          "reanchor-left",
          "reanchor-right",
          "selected-split-1"
        ]
      }
    },
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "old-gap-owner": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "reanchor-pause",
        "iterations": {
          "runs": [
            {
              "allocation": "old-configured-gap",
              "first": 0,
              "count": 1
            },
            {
              "allocation": "old-gap-growth",
              "first": 0,
              "count": 2
            },
            {
              "allocation": "binding-gap-growth",
              "first": 0,
              "count": 1
            }
          ]
        },
        "gap": {
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
    "reanchor-copy": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-left": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "selected-pause-hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-pause": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-right": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-copy",
        "duration": 1,
        "mapping": {
          "start": 1,
          "end": 2
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-split-1",
        "duration": 2,
        "mapping": {
          "start": 1,
          "end": 3
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-1": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "selected-pause-hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "selected-split-0": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-1": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "selected-split-2",
        "duration": 25,
        "mapping": {
          "start": 5,
          "end": 30
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-2": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "source": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "splice-copy-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "splice-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-2": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    }
  },
  "assets": {
    "audio": {
      "label": "Retained original audio fixture",
      "content_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "video": null,
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": null
    }
  },
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "splice",
      "origin": "hold"
    },
    "reanchor-copy": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "reanchor-split-1": {
      "allocation": "splice",
      "origin": "hold"
    },
    "selected-pause-hold": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "selected-split-2": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "source": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "splice-copy-2": {
      "allocation": "splice",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "append-reanchor",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 40,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "schema28-gap-capture",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 52,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "old-gap-owner",
                  "reanchor-split-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "old-gap-owner": {
              "duration": 12,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "repeat",
                "child": "reanchor-pause",
                "iterations": {
                  "runs": [
                    {
                      "allocation": "old-configured-gap",
                      "first": 0,
                      "count": 1
                    },
                    {
                      "allocation": "old-gap-growth",
                      "first": 0,
                      "count": 2
                    }
                  ]
                },
                "gap_duration": 3,
                "gap_audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-pause": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-0": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-split-1",
                "mapping": {
                  "start": 1,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-1": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "reanchor-split-1": {
              "allocation": "splice",
              "origin": "hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "selected-pause",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 38,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "source"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "splice",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "hold"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {}
        }
      }
    ],
    "bindings": {
      "hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          }
        ]
      },
      "reanchor-copy": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-copy"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "1",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "append-reanchor",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "reanchor-copy"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "1",
                  "denominator": "1"
                }
              }
            ]
          }
        }
      },
      "reanchor-pause": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-pause"
          },
          "arguments": [
            {
              "reference_repeat": "old-gap-owner",
              "value": {
                "type": "live",
                "repeat": "old-gap-owner"
              }
            }
          ],
          "births": [
            {
              "repeat": "old-gap-owner",
              "survivors": {
                "type": "captured_repeat",
                "repeat": "old-gap-owner"
              },
              "definition_root": "reanchor-pause"
            }
          ]
        },
        "resume": null
      },
      "reanchor-split-1": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "selected-pause-hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "selected-pause-hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "0",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": []
          }
        }
      },
      "selected-split-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "5",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "selected-pause",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "source"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "5",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "selected-split-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "source": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "splice-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "3",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "splice",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "hold"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "3",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "splice-copy-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      }
    },
    "gap_bindings": {
      "old-gap-owner": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "old-gap-owner",
            "recipe": "repeat_gap"
          },
          "gap_after": {
            "type": "live",
            "repeat": "old-gap-owner"
          },
          "arguments": [],
          "births": []
        },
        "resume": null
      }
    }
  }
}
');
INSERT INTO "revisions" VALUES('31dfda45-c4c0-40bd-ac25-7a226dcb1f9d','old-gap-rename','undo','{
  "schema_version": 22,
  "project_id": "d038df46-94c6-4c0a-9d11-038b0e6da220",
  "revision_id": "31dfda45-c4c0-40bd-ac25-7a226dcb1f9d",
  "presentation_basis": {
    "width": 16,
    "height": 16,
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
  "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
  "nodes": {
    "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "splice-copy-0",
          "old-gap-owner",
          "reanchor-split-0",
          "splice-copy-1",
          "selected-split-0",
          "reanchor-left",
          "reanchor-right",
          "selected-split-1"
        ]
      }
    },
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "old-gap-owner": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "reanchor-pause",
        "iterations": {
          "runs": [
            {
              "allocation": "old-configured-gap",
              "first": 0,
              "count": 1
            },
            {
              "allocation": "old-gap-growth",
              "first": 0,
              "count": 2
            },
            {
              "allocation": "binding-gap-growth",
              "first": 0,
              "count": 1
            }
          ]
        },
        "gap": {
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
    "reanchor-copy": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-left": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "selected-pause-hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-pause": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-right": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-copy",
        "duration": 1,
        "mapping": {
          "start": 1,
          "end": 2
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-split-1",
        "duration": 2,
        "mapping": {
          "start": 1,
          "end": 3
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-1": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "selected-pause-hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "selected-split-0": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-1": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "selected-split-2",
        "duration": 25,
        "mapping": {
          "start": 5,
          "end": 30
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-2": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "source": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "splice-copy-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "splice-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-2": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    }
  },
  "assets": {
    "audio": {
      "label": "Retained original audio fixture",
      "content_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "video": null,
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": null
    }
  },
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "splice",
      "origin": "hold"
    },
    "reanchor-copy": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "reanchor-split-1": {
      "allocation": "splice",
      "origin": "hold"
    },
    "selected-pause-hold": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "selected-split-2": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "source": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "splice-copy-2": {
      "allocation": "splice",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "append-reanchor",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 40,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "schema28-gap-capture",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 52,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "old-gap-owner",
                  "reanchor-split-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "old-gap-owner": {
              "duration": 12,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "repeat",
                "child": "reanchor-pause",
                "iterations": {
                  "runs": [
                    {
                      "allocation": "old-configured-gap",
                      "first": 0,
                      "count": 1
                    },
                    {
                      "allocation": "old-gap-growth",
                      "first": 0,
                      "count": 2
                    }
                  ]
                },
                "gap_duration": 3,
                "gap_audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-pause": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-0": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-split-1",
                "mapping": {
                  "start": 1,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-1": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "reanchor-split-1": {
              "allocation": "splice",
              "origin": "hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "selected-pause",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 38,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "source"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "splice",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "hold"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {}
        }
      }
    ],
    "bindings": {
      "hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          }
        ]
      },
      "reanchor-copy": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-copy"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "1",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "append-reanchor",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "reanchor-copy"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "1",
                  "denominator": "1"
                }
              }
            ]
          }
        }
      },
      "reanchor-pause": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-pause"
          },
          "arguments": [
            {
              "reference_repeat": "old-gap-owner",
              "value": {
                "type": "live",
                "repeat": "old-gap-owner"
              }
            }
          ],
          "births": [
            {
              "repeat": "old-gap-owner",
              "survivors": {
                "type": "captured_repeat",
                "repeat": "old-gap-owner"
              },
              "definition_root": "reanchor-pause"
            }
          ]
        },
        "resume": null
      },
      "reanchor-split-1": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "selected-pause-hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "selected-pause-hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "0",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": []
          }
        }
      },
      "selected-split-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "5",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "selected-pause",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "source"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "5",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "selected-split-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "source": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "splice-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "3",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "splice",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "hold"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "3",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "splice-copy-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      }
    },
    "gap_bindings": {
      "old-gap-owner": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "old-gap-owner",
            "recipe": "repeat_gap"
          },
          "gap_after": {
            "type": "live",
            "repeat": "old-gap-owner"
          },
          "arguments": [],
          "births": []
        },
        "resume": null
      }
    }
  }
}
');
INSERT INTO "revisions" VALUES('2ed04852-5d37-42ec-bcc7-b4f5901ab904','31dfda45-c4c0-40bd-ac25-7a226dcb1f9d','redo','{
  "schema_version": 22,
  "project_id": "d038df46-94c6-4c0a-9d11-038b0e6da220",
  "revision_id": "2ed04852-5d37-42ec-bcc7-b4f5901ab904",
  "presentation_basis": {
    "width": 16,
    "height": 16,
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
  "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
  "nodes": {
    "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
      "label": "Retained core 22 gap binding",
      "kind": {
        "type": "sequence",
        "children": [
          "splice-copy-0",
          "old-gap-owner",
          "reanchor-split-0",
          "splice-copy-1",
          "selected-split-0",
          "reanchor-left",
          "reanchor-right",
          "selected-split-1"
        ]
      }
    },
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "old-gap-owner": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "reanchor-pause",
        "iterations": {
          "runs": [
            {
              "allocation": "old-configured-gap",
              "first": 0,
              "count": 1
            },
            {
              "allocation": "old-gap-growth",
              "first": 0,
              "count": 2
            },
            {
              "allocation": "binding-gap-growth",
              "first": 0,
              "count": 1
            }
          ]
        },
        "gap": {
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
    "reanchor-copy": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-left": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "selected-pause-hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-pause": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-right": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-copy",
        "duration": 1,
        "mapping": {
          "start": 1,
          "end": 2
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-split-1",
        "duration": 2,
        "mapping": {
          "start": 1,
          "end": 3
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-1": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "selected-pause-hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "selected-split-0": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-1": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "selected-split-2",
        "duration": 25,
        "mapping": {
          "start": 5,
          "end": 30
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-2": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "source": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "splice-copy-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "splice-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-2": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    }
  },
  "assets": {
    "audio": {
      "label": "Retained original audio fixture",
      "content_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "video": null,
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": null
    }
  },
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "splice",
      "origin": "hold"
    },
    "reanchor-copy": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "reanchor-split-1": {
      "allocation": "splice",
      "origin": "hold"
    },
    "selected-pause-hold": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "selected-split-2": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "source": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "splice-copy-2": {
      "allocation": "splice",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "append-reanchor",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 40,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "schema28-gap-capture",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 52,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "old-gap-owner",
                  "reanchor-split-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "old-gap-owner": {
              "duration": 12,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "repeat",
                "child": "reanchor-pause",
                "iterations": {
                  "runs": [
                    {
                      "allocation": "old-configured-gap",
                      "first": 0,
                      "count": 1
                    },
                    {
                      "allocation": "old-gap-growth",
                      "first": 0,
                      "count": 2
                    }
                  ]
                },
                "gap_duration": 3,
                "gap_audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-pause": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-0": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-split-1",
                "mapping": {
                  "start": 1,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-1": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "reanchor-split-1": {
              "allocation": "splice",
              "origin": "hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "selected-pause",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 38,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "source"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "splice",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "hold"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {}
        }
      }
    ],
    "bindings": {
      "hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          }
        ]
      },
      "reanchor-copy": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-copy"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "1",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "append-reanchor",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "reanchor-copy"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "1",
                  "denominator": "1"
                }
              }
            ]
          }
        }
      },
      "reanchor-pause": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-pause"
          },
          "arguments": [
            {
              "reference_repeat": "old-gap-owner",
              "value": {
                "type": "live",
                "repeat": "old-gap-owner"
              }
            }
          ],
          "births": [
            {
              "repeat": "old-gap-owner",
              "survivors": {
                "type": "captured_repeat",
                "repeat": "old-gap-owner"
              },
              "definition_root": "reanchor-pause"
            }
          ]
        },
        "resume": null
      },
      "reanchor-split-1": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "selected-pause-hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "selected-pause-hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "0",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": []
          }
        }
      },
      "selected-split-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "5",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "selected-pause",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "source"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "5",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "selected-split-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "source": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "splice-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "3",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "splice",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "hold"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "3",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "splice-copy-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      }
    },
    "gap_bindings": {
      "old-gap-owner": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "old-gap-owner",
            "recipe": "repeat_gap"
          },
          "gap_after": {
            "type": "live",
            "repeat": "old-gap-owner"
          },
          "arguments": [],
          "births": []
        },
        "resume": null
      }
    }
  }
}
');
INSERT INTO "revisions" VALUES('75c86f20-9e26-4f1e-96f4-7cfbdcb39ba4','2ed04852-5d37-42ec-bcc7-b4f5901ab904','undo','{
  "schema_version": 22,
  "project_id": "d038df46-94c6-4c0a-9d11-038b0e6da220",
  "revision_id": "75c86f20-9e26-4f1e-96f4-7cfbdcb39ba4",
  "presentation_basis": {
    "width": 16,
    "height": 16,
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
  "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
  "nodes": {
    "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "splice-copy-0",
          "old-gap-owner",
          "reanchor-split-0",
          "splice-copy-1",
          "selected-split-0",
          "reanchor-left",
          "reanchor-right",
          "selected-split-1"
        ]
      }
    },
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "old-gap-owner": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "reanchor-pause",
        "iterations": {
          "runs": [
            {
              "allocation": "old-configured-gap",
              "first": 0,
              "count": 1
            },
            {
              "allocation": "old-gap-growth",
              "first": 0,
              "count": 2
            },
            {
              "allocation": "binding-gap-growth",
              "first": 0,
              "count": 1
            }
          ]
        },
        "gap": {
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
    "reanchor-copy": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-left": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "selected-pause-hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-pause": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "reanchor-right": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-copy",
        "duration": 1,
        "mapping": {
          "start": 1,
          "end": 2
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "reanchor-split-1",
        "duration": 2,
        "mapping": {
          "start": 1,
          "end": 3
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "reanchor-split-1": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    },
    "selected-pause-hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
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
    "selected-split-0": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "source",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-1": {
      "label": "Retained pending rename",
      "kind": {
        "type": "retime",
        "child": "selected-split-2",
        "duration": 25,
        "mapping": {
          "start": 5,
          "end": 30
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "selected-split-2": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "source": {
      "label": "Retained pending rename",
      "kind": {
        "type": "source",
        "source": {
          "duration": 30,
          "video": {
            "type": "blank"
          },
          "video_mapping": {
            "type": "fit_beat"
          },
          "audio": {
            "asset": "audio",
            "span": {
              "start": {
                "ticks": 0,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 48000,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "selected_placement",
            "start": {
              "numerator": "-2",
              "denominator": "3"
            },
            "frames": {
              "numerator": "30000",
              "denominator": "1001"
            },
            "selection": {
              "start": {
                "numerator": "2",
                "denominator": "1"
              },
              "end": {
                "numerator": "20",
                "denominator": "1"
              }
            }
          },
          "link": "independent",
          "audio_offset": -31
        }
      }
    },
    "splice-copy-0": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 1,
        "mapping": {
          "start": 0,
          "end": 1
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "splice-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "splice-copy-2": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 8,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      },
      "framing": {
        "value": {
          "type": "envelope",
          "envelope": {
            "initial": {
              "center_x": {
                "numerator": "1",
                "denominator": "2"
              },
              "center_y": {
                "numerator": "1",
                "denominator": "2"
              },
              "scale": {
                "numerator": "1",
                "denominator": "1"
              }
            },
            "segments": [
              {
                "end": {
                  "numerator": "1",
                  "denominator": "1"
                },
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
                    "numerator": "27",
                    "denominator": "20"
                  }
                },
                "curve": {
                  "type": "smoothstep"
                }
              }
            ]
          }
        }
      }
    }
  },
  "assets": {
    "audio": {
      "label": "Retained original audio fixture",
      "content_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "video": null,
      "audio": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 48000,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": null
    }
  },
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "splice",
      "origin": "hold"
    },
    "reanchor-copy": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "reanchor-split-1": {
      "allocation": "splice",
      "origin": "hold"
    },
    "selected-pause-hold": {
      "allocation": "split-reanchor",
      "origin": "selected-pause-hold"
    },
    "selected-split-2": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "source": {
      "allocation": "selected-pause",
      "origin": "source"
    },
    "splice-copy-2": {
      "allocation": "splice",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "append-reanchor",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 40,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "schema28-gap-capture",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 52,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "old-gap-owner",
                  "reanchor-split-0",
                  "splice-copy-1",
                  "selected-split-0",
                  "reanchor-left",
                  "reanchor-right",
                  "selected-split-1"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "old-gap-owner": {
              "duration": 12,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "repeat",
                "child": "reanchor-pause",
                "iterations": {
                  "runs": [
                    {
                      "allocation": "old-configured-gap",
                      "first": 0,
                      "count": 1
                    },
                    {
                      "allocation": "old-gap-growth",
                      "first": 0,
                      "count": 2
                    }
                  ]
                },
                "gap_duration": 3,
                "gap_audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-copy": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-left": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-pause-hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-pause": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "reanchor-right": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-copy",
                "mapping": {
                  "start": 1,
                  "end": 2
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-0": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "reanchor-split-1",
                "mapping": {
                  "start": 1,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "reanchor-split-1": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-pause-hold": {
              "duration": 2,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "selected-split-0": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "source",
                "mapping": {
                  "start": 0,
                  "end": 5
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-1": {
              "duration": 25,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "selected-split-2",
                "mapping": {
                  "start": 5,
                  "end": 30
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "selected-split-2": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 1,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 1
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "reanchor-copy": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "reanchor-split-1": {
              "allocation": "splice",
              "origin": "hold"
            },
            "selected-pause-hold": {
              "allocation": "split-reanchor",
              "origin": "selected-pause-hold"
            },
            "selected-split-2": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "source": {
              "allocation": "selected-pause",
              "origin": "source"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "selected-pause",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 38,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "splice-copy-0",
                  "splice-copy-1",
                  "source"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            },
            "source": {
              "duration": 30,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "source",
                "placement": {
                  "start": {
                    "numerator": "15861",
                    "denominator": "8008"
                  },
                  "end": {
                    "numerator": "160005",
                    "denominator": "8008"
                  }
                }
              }
            },
            "splice-copy-0": {
              "duration": 3,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "hold",
                "mapping": {
                  "start": 0,
                  "end": 3
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-1": {
              "duration": 5,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "retime",
                "child": "splice-copy-2",
                "mapping": {
                  "start": 3,
                  "end": 8
                },
                "pitch": "preserve",
                "purpose": "partition"
              }
            },
            "splice-copy-2": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {},
          "audio_lineage": {
            "hold": {
              "allocation": "splice",
              "origin": "hold"
            },
            "splice-copy-2": {
              "allocation": "splice",
              "origin": "hold"
            }
          }
        }
      },
      {
        "id": {
          "allocation": "splice",
          "ordinal": 0
        },
        "layout": {
          "root": "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "b5903f58-0249-4a89-8af8-ed3d8a5cfe1f": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "sequence",
                "children": [
                  "hold"
                ]
              }
            },
            "hold": {
              "duration": 8,
              "edges": {
                "node_start": "automatic",
                "node_end": "automatic",
                "source_placement_start": "automatic",
                "source_placement_end": "automatic",
                "repeat_gap_start": "automatic",
                "repeat_gap_end": "automatic"
              },
              "kind": {
                "type": "hold",
                "audio": {
                  "type": "silence"
                }
              }
            }
          },
          "overrides": {}
        }
      }
    ],
    "bindings": {
      "hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          }
        ]
      },
      "reanchor-copy": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-copy"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "1",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "append-reanchor",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "reanchor-copy"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "1",
                  "denominator": "1"
                }
              }
            ]
          }
        }
      },
      "reanchor-pause": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "reanchor-pause"
          },
          "arguments": [
            {
              "reference_repeat": "old-gap-owner",
              "value": {
                "type": "live",
                "repeat": "old-gap-owner"
              }
            }
          ],
          "births": [
            {
              "repeat": "old-gap-owner",
              "survivors": {
                "type": "captured_repeat",
                "repeat": "old-gap-owner"
              },
              "definition_root": "reanchor-pause"
            }
          ]
        },
        "resume": null
      },
      "reanchor-split-1": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "selected-pause-hold": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "append-reanchor",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "selected-pause-hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "0",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": []
          }
        }
      },
      "selected-split-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "5",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "selected-pause",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "source"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "5",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "selected-split-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "source": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "selected-pause",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "source"
          },
          "arguments": [],
          "births": []
        },
        "resume": null,
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "selected-pause",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "source"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      },
      "splice-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "splice",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "hold"
          },
          "arguments": [],
          "births": []
        },
        "resume": {
          "local_boundary": {
            "numerator": "3",
            "denominator": "1"
          },
          "phase": {
            "constant": {
              "numerator": "0",
              "denominator": "1"
            },
            "terms": [
              {
                "placement": {
                  "reference": {
                    "timing": {
                      "allocation": "splice",
                      "ordinal": 0
                    },
                    "root": {
                      "type": "project_root_round_even"
                    },
                    "physical": "hold"
                  },
                  "arguments": [],
                  "births": []
                },
                "from_local": {
                  "numerator": "0",
                  "denominator": "1"
                },
                "to_local": {
                  "numerator": "3",
                  "denominator": "1"
                }
              }
            ]
          }
        },
        "reanchors": [
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "splice",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "hold"
              },
              "arguments": [],
              "births": []
            }
          },
          {
            "placement": {
              "reference": {
                "timing": {
                  "allocation": "append-reanchor",
                  "ordinal": 0
                },
                "root": {
                  "type": "project_root_round_even"
                },
                "physical": "splice-copy-2"
              },
              "arguments": [],
              "births": []
            },
            "window": {
              "start": {
                "numerator": "1",
                "denominator": "1"
              },
              "end": {
                "numerator": "40",
                "denominator": "1"
              }
            }
          }
        ]
      }
    },
    "gap_bindings": {
      "old-gap-owner": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "schema28-gap-capture",
              "ordinal": 0
            },
            "root": {
              "type": "project_root_round_even"
            },
            "physical": "old-gap-owner",
            "recipe": "repeat_gap"
          },
          "gap_after": {
            "type": "live",
            "repeat": "old-gap-owner"
          },
          "arguments": [],
          "births": []
        },
        "resume": null
      }
    }
  }
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
INSERT INTO "state" VALUES(1,'75c86f20-9e26-4f1e-96f4-7cfbdcb39ba4',1,'generic');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
