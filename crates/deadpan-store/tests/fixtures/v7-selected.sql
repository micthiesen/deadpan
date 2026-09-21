-- Generated with Deadpan 63531f78799d5329f5c9fa7f246a9144f9dc156f (database schema 7, core schema 5).
-- Genuine old-binary fixture: authored rename/undo with pending redo and one selected legacy generation attempt.
PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO revisions VALUES('fixture-base',NULL,'initial',unistr('{\u000a  "schema_version": 5,\u000a  "project_id": "schema7-project",\u000a  "revision_id": "fixture-base",\u000a  "presentation_basis": {\u000a    "width": 512,\u000a    "height": 320,\u000a    "frame_rate": {\u000a      "numerator": 30,\u000a      "denominator": 1\u000a    },\u000a    "color_policy": "sdr_rec709"\u000a  },\u000a  "root": "root",\u000a  "nodes": {\u000a    "hold": {\u000a      "label": "Pause",\u000a      "kind": {\u000a        "type": "hold",\u000a        "recipe": {\u000a          "duration": 12,\u000a          "video": {\u000a            "type": "background"\u000a          },\u000a          "audio": {\u000a            "type": "silence"\u000a          }\u000a        }\u000a      }\u000a    },\u000a    "root": {\u000a      "label": "Sequence",\u000a      "kind": {\u000a        "type": "sequence",\u000a        "children": [\u000a          "hold"\u000a        ]\u000a      }\u000a    }\u000a  },\u000a  "assets": {},\u000a  "marks": {},\u000a  "overrides": {}\u000a}\u000a'));
INSERT INTO revisions VALUES('rename','fixture-base','edit',unistr('{\u000a  "schema_version": 5,\u000a  "project_id": "schema7-project",\u000a  "revision_id": "rename",\u000a  "presentation_basis": {\u000a    "width": 512,\u000a    "height": 320,\u000a    "frame_rate": {\u000a      "numerator": 30,\u000a      "denominator": 1\u000a    },\u000a    "color_policy": "sdr_rec709"\u000a  },\u000a  "root": "root",\u000a  "nodes": {\u000a    "hold": {\u000a      "label": "Renamed",\u000a      "kind": {\u000a        "type": "hold",\u000a        "recipe": {\u000a          "duration": 12,\u000a          "video": {\u000a            "type": "background"\u000a          },\u000a          "audio": {\u000a            "type": "silence"\u000a          }\u000a        }\u000a      }\u000a    },\u000a    "root": {\u000a      "label": "Sequence",\u000a      "kind": {\u000a        "type": "sequence",\u000a        "children": [\u000a          "hold"\u000a        ]\u000a      }\u000a    }\u000a  },\u000a  "assets": {},\u000a  "marks": {},\u000a  "overrides": {}\u000a}\u000a'));
INSERT INTO revisions VALUES('undo-rename','rename','undo',unistr('{\u000a  "schema_version": 5,\u000a  "project_id": "schema7-project",\u000a  "revision_id": "undo-rename",\u000a  "presentation_basis": {\u000a    "width": 512,\u000a    "height": 320,\u000a    "frame_rate": {\u000a      "numerator": 30,\u000a      "denominator": 1\u000a    },\u000a    "color_policy": "sdr_rec709"\u000a  },\u000a  "root": "root",\u000a  "nodes": {\u000a    "hold": {\u000a      "label": "Pause",\u000a      "kind": {\u000a        "type": "hold",\u000a        "recipe": {\u000a          "duration": 12,\u000a          "video": {\u000a            "type": "background"\u000a          },\u000a          "audio": {\u000a            "type": "silence"\u000a          }\u000a        }\u000a      }\u000a    },\u000a    "root": {\u000a      "label": "Sequence",\u000a      "kind": {\u000a        "type": "sequence",\u000a        "children": [\u000a          "hold"\u000a        ]\u000a      }\u000a    }\u000a  },\u000a  "assets": {},\u000a  "marks": {},\u000a  "overrides": {}\u000a}\u000a'));
CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
INSERT INTO history VALUES(1,NULL,'rename','{"project_id":"schema7-project","expected_revision":"fixture-base","new_revision":"rename","command":{"command":"rename","node":"hold","label":"Renamed"}}','{"forward":{"project_id":"schema7-project","from_revision":"fixture-base","to_revision":"rename","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Renamed","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"schema7-project","from_revision":"rename","to_revision":"fixture-base","nodes":{"hold":{"before":{"label":"Renamed","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["hold"],"duration_delta":0,"description":"Rename beat"}');
CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id)
        ) STRICT;
INSERT INTO state VALUES(1,'undo-rename',NULL);
CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
INSERT INTO redo VALUES(1,1);
CREATE TABLE hold_request_clocks (
    hold_id TEXT PRIMARY KEY,
    high_water INTEGER NOT NULL
        CHECK (high_water BETWEEN 1 AND 9223372036854775807)
) STRICT;
INSERT INTO hold_request_clocks VALUES('hold',1);
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
    relevance TEXT NOT NULL CHECK (relevance IN ('current','stale','detached')),
    UNIQUE (hold_id, request_version)
) STRICT;
INSERT INTO generation_requests VALUES('schema7-request','schema7-project','hold',1,'undo-rename','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":7}','current');
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
INSERT INTO generation_attempts VALUES('schema7-request','selected',1,'cancel-selected','ready','inference',6,NULL,'{"media":{"reference":"outputs/candidate.mp4","sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","byte_length":8192},"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"provider":{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":7}}',NULL,NULL,NULL);
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
INSERT INTO generation_candidate_receipts VALUES('schema7-request','selected','Candidates/schema7/selected.mp4','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',8192,'{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":7}','deadpan-media','1.0','present');
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
INSERT INTO generation_attempt_heads VALUES('schema7-request',1,'selected','selected');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
PRAGMA application_id=1146113585;
PRAGMA user_version=7;
