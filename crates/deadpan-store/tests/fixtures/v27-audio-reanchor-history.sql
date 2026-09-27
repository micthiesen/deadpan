PRAGMA application_id=1146113585;
PRAGMA user_version=27;
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
INSERT INTO "history" VALUES(1,NULL,'split-reanchor','{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","expected_revision":"45dfb0c5-4c46-4d8b-a878-1b29a1813635","new_revision":"split-reanchor","command":{"command":"split","node":"selected-pause-hold","at":1,"identities":{"nodes":["reanchor-left","reanchor-right","reanchor-copy"]}}}','{"forward":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"45dfb0c5-4c46-4d8b-a878-1b29a1813635","to_revision":"split-reanchor","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","selected-split-0","selected-pause-hold","selected-split-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}}},"reanchor-copy":{"before":null,"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"reanchor-left":{"before":null,"after":{"label":"Pause","kind":{"type":"retime","child":"selected-pause-hold","duration":1,"mapping":{"start":0,"end":1},"pitch":"preserve","purpose":"partition"}}},"reanchor-right":{"before":null,"after":{"label":"Pause","kind":{"type":"retime","child":"reanchor-copy","duration":1,"mapping":{"start":1,"end":2},"pitch":"preserve","purpose":"partition"}}}},"assets":{},"marks":{},"overrides":{},"audio_lineage":{"reanchor-copy":{"before":null,"after":{"allocation":"split-reanchor","origin":"selected-pause-hold"}},"selected-pause-hold":{"before":null,"after":{"allocation":"split-reanchor","origin":"selected-pause-hold"}}}},"inverse":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"split-reanchor","to_revision":"45dfb0c5-4c46-4d8b-a878-1b29a1813635","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","selected-split-0","selected-pause-hold","selected-split-1"]}}},"reanchor-copy":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"reanchor-left":{"before":{"label":"Pause","kind":{"type":"retime","child":"selected-pause-hold","duration":1,"mapping":{"start":0,"end":1},"pitch":"preserve","purpose":"partition"}},"after":null},"reanchor-right":{"before":{"label":"Pause","kind":{"type":"retime","child":"reanchor-copy","duration":1,"mapping":{"start":1,"end":2},"pitch":"preserve","purpose":"partition"}},"after":null}},"assets":{},"marks":{},"overrides":{},"audio_lineage":{"reanchor-copy":{"before":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"after":null},"selected-pause-hold":{"before":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"after":null}}},"changed_ids":["b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","reanchor-copy","reanchor-left","reanchor-right","selected-pause-hold"],"duration_delta":0,"description":"Split beat"}');
INSERT INTO "history" VALUES(2,1,'append-reanchor','{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","expected_revision":"split-reanchor","new_revision":"append-reanchor","command":{"command":"insert_time","at":1,"hold":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}},"id":"reanchor-pause","identities":{"nodes":["reanchor-split-0","reanchor-split-1","reanchor-split-2","reanchor-split-3","reanchor-split-4","reanchor-split-5","reanchor-split-6","reanchor-split-7","reanchor-split-8","reanchor-split-9","reanchor-split-10","reanchor-split-11","reanchor-split-12","reanchor-split-13","reanchor-split-14","reanchor-split-15","reanchor-split-16"]},"timing":{"allocation":"append-reanchor","ordinal":0}}}','{"forward":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"split-reanchor","to_revision":"append-reanchor","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","reanchor-pause","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}}},"reanchor-pause":{"before":null,"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"reanchor-split-0":{"before":null,"after":{"label":"Pause","kind":{"type":"retime","child":"reanchor-split-1","duration":2,"mapping":{"start":1,"end":3},"pitch":"preserve","purpose":"partition"}}},"reanchor-split-1":{"before":null,"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}}},"framing":{"value":{"type":"envelope","envelope":{"initial":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"1","denominator":"1"}},"segments":[{"end":{"numerator":"1","denominator":"1"},"pose":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"27","denominator":"20"}},"curve":{"type":"smoothstep"}}]}}}}},"splice-copy-0":{"before":{"label":"Pause","kind":{"type":"retime","child":"hold","duration":3,"mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"after":{"label":"Pause","kind":{"type":"retime","child":"hold","duration":1,"mapping":{"start":0,"end":1},"pitch":"preserve","purpose":"partition"}}}},"assets":{},"marks":{},"overrides":{},"audio_lineage":{"reanchor-split-1":{"before":null,"after":{"allocation":"splice","origin":"hold"}}},"audio_bindings":{"before":{"timings":[{"id":{"allocation":"selected-pause","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":38,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","source"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"source":{"duration":30,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"source","placement":{"start":{"numerator":"15861","denominator":"8008"},"end":{"numerator":"160005","denominator":"8008"}}}},"splice-copy-0":{"duration":3,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"hold","mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"splice-copy-1":{"duration":5,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"splice-copy-2","mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}},"splice-copy-2":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{},"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"splice-copy-2":{"allocation":"splice","origin":"hold"}}}},{"id":{"allocation":"splice","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["hold"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{}}}],"bindings":{"hold":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}}]},"selected-split-2":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"5","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"5","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}}]},"source":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}}]},"splice-copy-2":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"3","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"3","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}}]}}},"after":{"timings":[{"id":{"allocation":"append-reanchor","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":40,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"reanchor-copy":{"duration":2,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"reanchor-left":{"duration":1,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"selected-pause-hold","mapping":{"start":0,"end":1},"pitch":"preserve","purpose":"partition"}},"reanchor-right":{"duration":1,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"reanchor-copy","mapping":{"start":1,"end":2},"pitch":"preserve","purpose":"partition"}},"selected-pause-hold":{"duration":2,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"selected-split-0":{"duration":5,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"source","mapping":{"start":0,"end":5},"pitch":"preserve","purpose":"partition"}},"selected-split-1":{"duration":25,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"selected-split-2","mapping":{"start":5,"end":30},"pitch":"preserve","purpose":"partition"}},"selected-split-2":{"duration":30,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"source","placement":{"start":{"numerator":"15861","denominator":"8008"},"end":{"numerator":"160005","denominator":"8008"}}}},"source":{"duration":30,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"source","placement":{"start":{"numerator":"15861","denominator":"8008"},"end":{"numerator":"160005","denominator":"8008"}}}},"splice-copy-0":{"duration":3,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"hold","mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"splice-copy-1":{"duration":5,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"splice-copy-2","mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}},"splice-copy-2":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{},"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"reanchor-copy":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"selected-pause-hold":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"selected-split-2":{"allocation":"selected-pause","origin":"source"},"source":{"allocation":"selected-pause","origin":"source"},"splice-copy-2":{"allocation":"splice","origin":"hold"}}}},{"id":{"allocation":"selected-pause","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":38,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","source"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"source":{"duration":30,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"source","placement":{"start":{"numerator":"15861","denominator":"8008"},"end":{"numerator":"160005","denominator":"8008"}}}},"splice-copy-0":{"duration":3,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"hold","mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"splice-copy-1":{"duration":5,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"splice-copy-2","mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}},"splice-copy-2":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{},"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"splice-copy-2":{"allocation":"splice","origin":"hold"}}}},{"id":{"allocation":"splice","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["hold"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{}}}],"bindings":{"hold":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}}]},"reanchor-copy":{"lattice":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"reanchor-copy"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"1","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"reanchor-copy"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"1","denominator":"1"}}]}}},"reanchor-split-1":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}},{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"window":{"start":{"numerator":"1","denominator":"1"},"end":{"numerator":"40","denominator":"1"}}}]},"selected-pause-hold":{"lattice":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"selected-pause-hold"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"0","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[]}}},"selected-split-2":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"5","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"5","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}},{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"selected-split-2"},"arguments":[],"births":[]},"window":{"start":{"numerator":"1","denominator":"1"},"end":{"numerator":"40","denominator":"1"}}}]},"source":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}},{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"window":{"start":{"numerator":"1","denominator":"1"},"end":{"numerator":"40","denominator":"1"}}}]},"splice-copy-2":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"3","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"3","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}},{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"splice-copy-2"},"arguments":[],"births":[]},"window":{"start":{"numerator":"1","denominator":"1"},"end":{"numerator":"40","denominator":"1"}}}]}}}}},"inverse":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"append-reanchor","to_revision":"split-reanchor","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","reanchor-pause","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}}},"reanchor-pause":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"reanchor-split-0":{"before":{"label":"Pause","kind":{"type":"retime","child":"reanchor-split-1","duration":2,"mapping":{"start":1,"end":3},"pitch":"preserve","purpose":"partition"}},"after":null},"reanchor-split-1":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}}},"framing":{"value":{"type":"envelope","envelope":{"initial":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"1","denominator":"1"}},"segments":[{"end":{"numerator":"1","denominator":"1"},"pose":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"27","denominator":"20"}},"curve":{"type":"smoothstep"}}]}}}},"after":null},"splice-copy-0":{"before":{"label":"Pause","kind":{"type":"retime","child":"hold","duration":1,"mapping":{"start":0,"end":1},"pitch":"preserve","purpose":"partition"}},"after":{"label":"Pause","kind":{"type":"retime","child":"hold","duration":3,"mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}}}},"assets":{},"marks":{},"overrides":{},"audio_lineage":{"reanchor-split-1":{"before":{"allocation":"splice","origin":"hold"},"after":null}},"audio_bindings":{"before":{"timings":[{"id":{"allocation":"append-reanchor","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":40,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"reanchor-copy":{"duration":2,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"reanchor-left":{"duration":1,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"selected-pause-hold","mapping":{"start":0,"end":1},"pitch":"preserve","purpose":"partition"}},"reanchor-right":{"duration":1,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"reanchor-copy","mapping":{"start":1,"end":2},"pitch":"preserve","purpose":"partition"}},"selected-pause-hold":{"duration":2,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"selected-split-0":{"duration":5,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"source","mapping":{"start":0,"end":5},"pitch":"preserve","purpose":"partition"}},"selected-split-1":{"duration":25,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"selected-split-2","mapping":{"start":5,"end":30},"pitch":"preserve","purpose":"partition"}},"selected-split-2":{"duration":30,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"source","placement":{"start":{"numerator":"15861","denominator":"8008"},"end":{"numerator":"160005","denominator":"8008"}}}},"source":{"duration":30,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"source","placement":{"start":{"numerator":"15861","denominator":"8008"},"end":{"numerator":"160005","denominator":"8008"}}}},"splice-copy-0":{"duration":3,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"hold","mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"splice-copy-1":{"duration":5,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"splice-copy-2","mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}},"splice-copy-2":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{},"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"reanchor-copy":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"selected-pause-hold":{"allocation":"split-reanchor","origin":"selected-pause-hold"},"selected-split-2":{"allocation":"selected-pause","origin":"source"},"source":{"allocation":"selected-pause","origin":"source"},"splice-copy-2":{"allocation":"splice","origin":"hold"}}}},{"id":{"allocation":"selected-pause","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":38,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","source"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"source":{"duration":30,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"source","placement":{"start":{"numerator":"15861","denominator":"8008"},"end":{"numerator":"160005","denominator":"8008"}}}},"splice-copy-0":{"duration":3,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"hold","mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"splice-copy-1":{"duration":5,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"splice-copy-2","mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}},"splice-copy-2":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{},"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"splice-copy-2":{"allocation":"splice","origin":"hold"}}}},{"id":{"allocation":"splice","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["hold"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{}}}],"bindings":{"hold":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}}]},"reanchor-copy":{"lattice":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"reanchor-copy"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"1","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"reanchor-copy"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"1","denominator":"1"}}]}}},"reanchor-split-1":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}},{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"window":{"start":{"numerator":"1","denominator":"1"},"end":{"numerator":"40","denominator":"1"}}}]},"selected-pause-hold":{"lattice":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"selected-pause-hold"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"0","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[]}}},"selected-split-2":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"5","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"5","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}},{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"selected-split-2"},"arguments":[],"births":[]},"window":{"start":{"numerator":"1","denominator":"1"},"end":{"numerator":"40","denominator":"1"}}}]},"source":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}},{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"window":{"start":{"numerator":"1","denominator":"1"},"end":{"numerator":"40","denominator":"1"}}}]},"splice-copy-2":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"3","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"3","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}},{"placement":{"reference":{"timing":{"allocation":"append-reanchor","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"splice-copy-2"},"arguments":[],"births":[]},"window":{"start":{"numerator":"1","denominator":"1"},"end":{"numerator":"40","denominator":"1"}}}]}}},"after":{"timings":[{"id":{"allocation":"selected-pause","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":38,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","source"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"source":{"duration":30,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"source","placement":{"start":{"numerator":"15861","denominator":"8008"},"end":{"numerator":"160005","denominator":"8008"}}}},"splice-copy-0":{"duration":3,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"hold","mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"splice-copy-1":{"duration":5,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"splice-copy-2","mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}},"splice-copy-2":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{},"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"splice-copy-2":{"allocation":"splice","origin":"hold"}}}},{"id":{"allocation":"splice","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["hold"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{}}}],"bindings":{"hold":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}}]},"selected-split-2":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"5","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"5","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}}]},"source":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}}]},"splice-copy-2":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"3","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"3","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}}]}}}}},"changed_ids":["b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","reanchor-copy","reanchor-pause","reanchor-split-0","reanchor-split-1","selected-pause-hold","selected-split-2","source","splice-copy-0","splice-copy-2"],"duration_delta":2,"description":"Insert pause"}');
INSERT INTO "history" VALUES(3,2,'old-configured-gap','{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","expected_revision":"append-reanchor","new_revision":"old-configured-gap","command":{"command":"wrap_repeat","node":"reanchor-pause","id":"old-gap-owner","plays":1,"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}},"anchor_policy":"first"}}','{"forward":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"append-reanchor","to_revision":"old-configured-gap","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","reanchor-pause","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}}},"old-gap-owner":{"before":null,"after":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"old-configured-gap","to_revision":"append-reanchor","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","reanchor-pause","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}}},"old-gap-owner":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","old-gap-owner"],"duration_delta":0,"description":"Wrap repeat"}');
INSERT INTO "history" VALUES(4,3,'old-gap-growth','{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","expected_revision":"old-configured-gap","new_revision":"old-gap-growth","command":{"command":"set_repeat","node":"old-gap-owner","plays":3,"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}}','{"forward":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"old-configured-gap","to_revision":"old-gap-growth","nodes":{"old-gap-owner":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1},{"allocation":"old-gap-growth","first":0,"count":2}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"old-gap-growth","to_revision":"old-configured-gap","nodes":{"old-gap-owner":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1},{"allocation":"old-gap-growth","first":0,"count":2}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Repeat","kind":{"type":"repeat","child":"reanchor-pause","iterations":{"runs":[{"allocation":"old-configured-gap","first":0,"count":1}]},"gap":{"duration":3,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["old-gap-owner"],"duration_delta":10,"description":"Set repeat parameters"}');
INSERT INTO "history" VALUES(5,4,'pending-reanchor-rename','{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","expected_revision":"old-gap-growth","new_revision":"pending-reanchor-rename","command":{"command":"rename","node":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","label":"Retained reanchor redo"}}','{"forward":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"old-gap-growth","to_revision":"pending-reanchor-rename","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"after":{"label":"Retained reanchor redo","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","from_revision":"pending-reanchor-rename","to_revision":"old-gap-growth","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"before":{"label":"Retained reanchor redo","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","old-gap-owner","reanchor-split-0","splice-copy-1","selected-split-0","reanchor-left","reanchor-right","selected-split-1"]}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["b5903f58-0249-4a89-8af8-ed3d8a5cfe1f"],"duration_delta":0,"description":"Rename beat"}');
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
INSERT INTO "redo" VALUES(1,5);
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('45dfb0c5-4c46-4d8b-a878-1b29a1813635',NULL,'initial','{"schema_version":21,"project_id":"77950d7f-03cd-4bad-ae5d-f7ffe750e234","revision_id":"45dfb0c5-4c46-4d8b-a878-1b29a1813635","presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"label":"Sequence","kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","selected-split-0","selected-pause-hold","selected-split-1"]}},"hold":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}}},"framing":{"value":{"type":"envelope","envelope":{"initial":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"1","denominator":"1"}},"segments":[{"end":{"numerator":"1","denominator":"1"},"pose":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"27","denominator":"20"}},"curve":{"type":"smoothstep"}}]}}}},"selected-pause-hold":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}},"selected-split-0":{"label":"Retained pending rename","kind":{"type":"retime","child":"source","duration":5,"mapping":{"start":0,"end":5},"pitch":"preserve","purpose":"partition"}},"selected-split-1":{"label":"Retained pending rename","kind":{"type":"retime","child":"selected-split-2","duration":25,"mapping":{"start":5,"end":30},"pitch":"preserve","purpose":"partition"}},"selected-split-2":{"label":"Retained pending rename","kind":{"type":"source","source":{"duration":30,"video":{"type":"blank"},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"audio","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":48000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"selected_placement","start":{"numerator":"-2","denominator":"3"},"frames":{"numerator":"30000","denominator":"1001"},"selection":{"start":{"numerator":"2","denominator":"1"},"end":{"numerator":"20","denominator":"1"}}},"link":"independent","audio_offset":-31}}},"source":{"label":"Retained pending rename","kind":{"type":"source","source":{"duration":30,"video":{"type":"blank"},"video_mapping":{"type":"fit_beat"},"audio":{"asset":"audio","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":48000,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"selected_placement","start":{"numerator":"-2","denominator":"3"},"frames":{"numerator":"30000","denominator":"1001"},"selection":{"start":{"numerator":"2","denominator":"1"},"end":{"numerator":"20","denominator":"1"}}},"link":"independent","audio_offset":-31}}},"splice-copy-0":{"label":"Pause","kind":{"type":"retime","child":"hold","duration":3,"mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"splice-copy-1":{"label":"Pause","kind":{"type":"retime","child":"splice-copy-2","duration":5,"mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}},"splice-copy-2":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}}},"framing":{"value":{"type":"envelope","envelope":{"initial":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"1","denominator":"1"}},"segments":[{"end":{"numerator":"1","denominator":"1"},"pose":{"center_x":{"numerator":"1","denominator":"2"},"center_y":{"numerator":"1","denominator":"2"},"scale":{"numerator":"27","denominator":"20"}},"curve":{"type":"smoothstep"}}]}}}}},"assets":{"audio":{"label":"Retained original audio fixture","content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","video":null,"audio":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":48000,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":null}},"marks":{},"overrides":{},"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"selected-split-2":{"allocation":"selected-pause","origin":"source"},"source":{"allocation":"selected-pause","origin":"source"},"splice-copy-2":{"allocation":"splice","origin":"hold"}},"audio_bindings":{"timings":[{"id":{"allocation":"selected-pause","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":38,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["splice-copy-0","splice-copy-1","source"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}},"source":{"duration":30,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"source","placement":{"start":{"numerator":"15861","denominator":"8008"},"end":{"numerator":"160005","denominator":"8008"}}}},"splice-copy-0":{"duration":3,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"hold","mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"splice-copy-1":{"duration":5,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"retime","child":"splice-copy-2","mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}},"splice-copy-2":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{},"audio_lineage":{"hold":{"allocation":"splice","origin":"hold"},"splice-copy-2":{"allocation":"splice","origin":"hold"}}}},{"id":{"allocation":"splice","ordinal":0},"layout":{"root":"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f","rate":{"numerator":30000,"denominator":1001},"nodes":{"b5903f58-0249-4a89-8af8-ed3d8a5cfe1f":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["hold"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{}}}],"bindings":{"hold":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}}]},"selected-split-2":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"5","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"5","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}}]},"source":{"lattice":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]},"resume":null,"reanchors":[{"placement":{"reference":{"timing":{"allocation":"selected-pause","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"source"},"arguments":[],"births":[]}}]},"splice-copy-2":{"lattice":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"3","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"3","denominator":"1"}}]}},"reanchors":[{"placement":{"reference":{"timing":{"allocation":"splice","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]}}]}}}}');
INSERT INTO "revisions" VALUES('split-reanchor','45dfb0c5-4c46-4d8b-a878-1b29a1813635','edit','{
  "schema_version": 21,
  "project_id": "77950d7f-03cd-4bad-ae5d-f7ffe750e234",
  "revision_id": "split-reanchor",
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
        "duration": 3,
        "mapping": {
          "start": 0,
          "end": 3
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
          }
        ]
      }
    }
  }
}
');
INSERT INTO "revisions" VALUES('append-reanchor','split-reanchor','edit','{
  "schema_version": 21,
  "project_id": "77950d7f-03cd-4bad-ae5d-f7ffe750e234",
  "revision_id": "append-reanchor",
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
          "reanchor-pause",
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
    }
  }
}
');
INSERT INTO "revisions" VALUES('old-configured-gap','append-reanchor','edit','{
  "schema_version": 21,
  "project_id": "77950d7f-03cd-4bad-ae5d-f7ffe750e234",
  "revision_id": "old-configured-gap",
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
    }
  }
}
');
INSERT INTO "revisions" VALUES('old-gap-growth','old-configured-gap','edit','{
  "schema_version": 21,
  "project_id": "77950d7f-03cd-4bad-ae5d-f7ffe750e234",
  "revision_id": "old-gap-growth",
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
    }
  }
}
');
INSERT INTO "revisions" VALUES('pending-reanchor-rename','old-gap-growth','edit','{
  "schema_version": 21,
  "project_id": "77950d7f-03cd-4bad-ae5d-f7ffe750e234",
  "revision_id": "pending-reanchor-rename",
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
      "label": "Retained reanchor redo",
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
    }
  }
}
');
INSERT INTO "revisions" VALUES('a3f0095f-2caa-435f-971e-d5780baa3584','pending-reanchor-rename','undo','{
  "schema_version": 21,
  "project_id": "77950d7f-03cd-4bad-ae5d-f7ffe750e234",
  "revision_id": "a3f0095f-2caa-435f-971e-d5780baa3584",
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
    }
  }
}
');
INSERT INTO "revisions" VALUES('c406ab3c-b89f-487f-b696-901905db1aaa','a3f0095f-2caa-435f-971e-d5780baa3584','redo','{
  "schema_version": 21,
  "project_id": "77950d7f-03cd-4bad-ae5d-f7ffe750e234",
  "revision_id": "c406ab3c-b89f-487f-b696-901905db1aaa",
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
      "label": "Retained reanchor redo",
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
    }
  }
}
');
INSERT INTO "revisions" VALUES('8acbf1bc-da27-4e45-b5dc-9dc3017175f1','c406ab3c-b89f-487f-b696-901905db1aaa','undo','{
  "schema_version": 21,
  "project_id": "77950d7f-03cd-4bad-ae5d-f7ffe750e234",
  "revision_id": "8acbf1bc-da27-4e45-b5dc-9dc3017175f1",
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
INSERT INTO "state" VALUES(1,'8acbf1bc-da27-4e45-b5dc-9dc3017175f1',4,'generic');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
