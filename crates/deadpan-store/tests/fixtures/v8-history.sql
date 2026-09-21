-- Genuine schema-8 fixture produced by commit a9555c6442c44a3e36b516bea442234329871606.
-- Contains a selected protocol-2 Ready bundle without admission evidence,
-- an authored edit, and pending redo. Fixture media bytes are synthetic.
PRAGMA application_id=1146113585;
PRAGMA user_version=8;
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
INSERT INTO "generation_attempt_heads" VALUES('schema8-ready',1,'schema8-attempt','schema8-attempt');
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
INSERT INTO "generation_attempts" VALUES('schema8-ready','schema8-attempt',1,'cancel-schema8-attempt','ready','inference',5,NULL,'{"native":{"reference":"outputs/native.mp4","sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","byte_length":101},"provenance":{"reference":"outputs/provenance.json","sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","byte_length":202},"video":{"frames":11,"frame_rate":{"numerator":24,"denominator":1},"width":512,"height":320},"provider":{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":1}}',NULL,NULL,NULL);
CREATE TABLE generation_bundle_receipts (
    request_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    bundle TEXT NOT NULL CHECK (json_valid(bundle)),
    availability TEXT NOT NULL CHECK (availability IN ('present','evicted')),
    PRIMARY KEY (request_id,attempt_id),
    FOREIGN KEY (request_id,attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;
INSERT INTO "generation_bundle_receipts" VALUES('schema8-ready','schema8-attempt','{"native_object":{"content":{"algorithm":"blake3","digest":"963e68ee6cfaa912e9fd9177f104492303a1ad57cfb623c420d6c50fae1b4778"},"byte_length":24},"sampled_object":{"content":{"algorithm":"blake3","digest":"c400ad14c7567f86598d53b98c079072f3fd3c1902ee28d09f04b0859a244716"},"byte_length":25},"provenance_object":{"content":{"algorithm":"blake3","digest":"1a4058ea089cda9f7d07a35e9c3ff6d1edc8b531bc74c3eb1de42b106d436ce5"},"byte_length":26},"native_video":{"frames":11,"frame_rate":{"numerator":24,"denominator":1},"width":512,"height":320},"sampled_video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"plan":{"schema_version":1,"operation":"bridge","interpolation":"linear","project":{"interior_frames":12,"frame_rate":{"numerator":30,"denominator":1}},"native":{"frame_count":11,"frame_rate":{"numerator":24,"denominator":1},"width":512,"height":320},"timing":{"requested_boundary_duration":{"numerator":"13","denominator":"30"},"actual_boundary_duration":{"numerator":"5","denominator":"12"},"retime_deviation":{"numerator":"-1","denominator":"60"}},"sampling":{"endpoint_policy":"interior_only"}},"provider":{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":1},"native_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","native_byte_length":101,"provenance_sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","provenance_byte_length":202,"validator":{"id":"deadpan-media","version":"bridge-1"},"availability":"present"}','present');
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
INSERT INTO "generation_requests" VALUES('schema8-ready','project','hold',1,'setup','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":1}','{"schema_version":1,"operation":"bridge","interpolation":"linear","project":{"interior_frames":12,"frame_rate":{"numerator":30,"denominator":1}},"native":{"frame_count":11,"frame_rate":{"numerator":24,"denominator":1},"width":512,"height":320},"timing":{"requested_boundary_duration":{"numerator":"13","denominator":"30"},"actual_boundary_duration":{"numerator":"5","denominator":"12"},"retime_deviation":{"numerator":"-1","denominator":"60"}},"sampling":{"endpoint_policy":"interior_only"}}','current');
CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
INSERT INTO "history" VALUES(1,NULL,'schema8-edit','{"project_id":"project","expected_revision":"setup","new_revision":"schema8-edit","command":{"command":"rename","node":"hold","label":"Renamed"}}','{"forward":{"project_id":"project","from_revision":"setup","to_revision":"schema8-edit","nodes":{"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Renamed","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project","from_revision":"schema8-edit","to_revision":"setup","nodes":{"hold":{"before":{"label":"Renamed","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["hold"],"duration_delta":0,"description":"Rename beat"}');
CREATE TABLE hold_request_clocks (
    hold_id TEXT PRIMARY KEY,
    high_water INTEGER NOT NULL
        CHECK (high_water BETWEEN 1 AND 9223372036854775807)
) STRICT;
INSERT INTO "hold_request_clocks" VALUES('hold',1);
CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
INSERT INTO "redo" VALUES(1,1);
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('setup',NULL,'initial','{
  "schema_version": 5,
  "project_id": "project",
  "revision_id": "setup",
  "presentation_basis": {
    "width": 512,
    "height": 320,
    "frame_rate": {
      "numerator": 30,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 12,
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
INSERT INTO "revisions" VALUES('schema8-edit','setup','edit','{
  "schema_version": 5,
  "project_id": "project",
  "revision_id": "schema8-edit",
  "presentation_basis": {
    "width": 512,
    "height": 320,
    "frame_rate": {
      "numerator": 30,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Renamed",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 12,
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
INSERT INTO "revisions" VALUES('schema8-undo','schema8-edit','undo','{
  "schema_version": 5,
  "project_id": "project",
  "revision_id": "schema8-undo",
  "presentation_basis": {
    "width": 512,
    "height": 320,
    "frame_rate": {
      "numerator": 30,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "hold": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 12,
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
CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id)
        ) STRICT;
INSERT INTO "state" VALUES(1,'schema8-undo',NULL);
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
