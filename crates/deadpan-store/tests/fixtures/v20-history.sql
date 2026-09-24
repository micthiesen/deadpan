-- Authentic database schema 20 / core schema 14 history.
-- Captured with the preserved CLI from revision 1eb043c on 2026-09-23.
-- CLI SHA-256: e37c13033c49ed47a7c980e6fc952f75e3d3ab160fc41aeaea6433cd40ffecad.
-- Direct Split, undo/redo, Repeat occurrence isolation, occurrence Split,
-- deletion and pending redo. Validated by the old executable and captured
-- through SQLite backup. No external media or local paths.
PRAGMA application_id=1146113585;
PRAGMA user_version=20;
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
INSERT INTO "history" VALUES(1,NULL,'insert','{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","expected_revision":"88c97e92-08a2-4548-9a9e-6a69bd7b557c","new_revision":"insert","command":{"command":"insert","parent":"14cc5d97-f828-4b9f-8f19-9dd49865253a","index":0,"subtree":{"root":"hold","nodes":{"hold":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"overrides":{}}}}','{"forward":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"88c97e92-08a2-4548-9a9e-6a69bd7b557c","to_revision":"insert","nodes":{"14cc5d97-f828-4b9f-8f19-9dd49865253a":{"before":{"label":"Sequence","kind":{"type":"sequence","children":[]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}}},"hold":{"before":null,"after":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"insert","to_revision":"88c97e92-08a2-4548-9a9e-6a69bd7b557c","nodes":{"14cc5d97-f828-4b9f-8f19-9dd49865253a":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":[]}}},"hold":{"before":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["14cc5d97-f828-4b9f-8f19-9dd49865253a","hold"],"duration_delta":12,"description":"Insert beats"}');
INSERT INTO "history" VALUES(2,1,'split','{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","expected_revision":"insert","new_revision":"split","command":{"command":"split","node":"hold","at":5,"identities":{"nodes":["left","right","right-context"]}}}','{"forward":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"insert","to_revision":"split","nodes":{"14cc5d97-f828-4b9f-8f19-9dd49865253a":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["left","right"]}}},"left":{"before":null,"after":{"label":"Silence","kind":{"type":"retime","child":"hold","duration":5,"mapping":{"start":0,"end":5},"pitch":"preserve","purpose":"partition"}}},"right":{"before":null,"after":{"label":"Silence","kind":{"type":"retime","child":"right-context","duration":7,"mapping":{"start":5,"end":12},"pitch":"preserve","purpose":"partition"}}},"right-context":{"before":null,"after":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"split","to_revision":"insert","nodes":{"14cc5d97-f828-4b9f-8f19-9dd49865253a":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["left","right"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}}},"left":{"before":{"label":"Silence","kind":{"type":"retime","child":"hold","duration":5,"mapping":{"start":0,"end":5},"pitch":"preserve","purpose":"partition"}},"after":null},"right":{"before":{"label":"Silence","kind":{"type":"retime","child":"right-context","duration":7,"mapping":{"start":5,"end":12},"pitch":"preserve","purpose":"partition"}},"after":null},"right-context":{"before":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["14cc5d97-f828-4b9f-8f19-9dd49865253a","left","right","right-context"],"duration_delta":0,"description":"Split beat"}');
INSERT INTO "history" VALUES(3,2,'wrap','{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","expected_revision":"68db4c7a-70f5-44be-9c4c-058de9dc0f70","new_revision":"wrap","command":{"command":"wrap_repeat","node":"right","id":"repeat","plays":3,"gap":null,"anchor_policy":"first"}}','{"forward":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"68db4c7a-70f5-44be-9c4c-058de9dc0f70","to_revision":"wrap","nodes":{"14cc5d97-f828-4b9f-8f19-9dd49865253a":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["left","right"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["left","repeat"]}}},"repeat":{"before":null,"after":{"label":"Repeat","kind":{"type":"repeat","child":"right","iterations":{"runs":[{"allocation":"wrap","first":0,"count":3}]},"gap":null}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"wrap","to_revision":"68db4c7a-70f5-44be-9c4c-058de9dc0f70","nodes":{"14cc5d97-f828-4b9f-8f19-9dd49865253a":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["left","repeat"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["left","right"]}}},"repeat":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"right","iterations":{"runs":[{"allocation":"wrap","first":0,"count":3}]},"gap":null}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["14cc5d97-f828-4b9f-8f19-9dd49865253a","repeat"],"duration_delta":14,"description":"Wrap repeat"}');
INSERT INTO "history" VALUES(4,3,'occurrence-rename','{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","expected_revision":"wrap","new_revision":"occurrence-rename","command":{"command":"edit_occurrence","instance":{"node":"right-context","repeats":[{"node":"repeat","iteration":{"allocation":"wrap","ordinal":1}}]},"edit":{"type":"rename","label":"Independent play"},"identities":{"nodes":["isolated-right","isolated-context"],"marks":[]}}}','{"forward":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"wrap","to_revision":"occurrence-rename","nodes":{"isolated-context":{"before":null,"after":{"label":"Independent play","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"isolated-right":{"before":null,"after":{"label":"Silence","kind":{"type":"retime","child":"isolated-context","duration":7,"mapping":{"start":5,"end":12},"pitch":"preserve","purpose":"partition"}}}},"assets":{},"marks":{},"overrides":{"repeat":{"before":null,"after":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated-right"}]}}},"inverse":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"occurrence-rename","to_revision":"wrap","nodes":{"isolated-context":{"before":{"label":"Independent play","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"isolated-right":{"before":{"label":"Silence","kind":{"type":"retime","child":"isolated-context","duration":7,"mapping":{"start":5,"end":12},"pitch":"preserve","purpose":"partition"}},"after":null}},"assets":{},"marks":{},"overrides":{"repeat":{"before":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated-right"}],"after":null}}},"changed_ids":["isolated-context","isolated-right","repeat"],"duration_delta":0,"description":"Edit selected occurrence"}');
INSERT INTO "history" VALUES(5,4,'occurrence-split','{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","expected_revision":"occurrence-rename","new_revision":"occurrence-split","command":{"command":"edit_occurrence","instance":{"node":"isolated-right","repeats":[{"node":"repeat","iteration":{"allocation":"wrap","ordinal":1}}]},"edit":{"type":"split","at":3,"identities":{"nodes":["isolated-tail","isolated-tail-context","isolated-sequence"]}},"identities":{"nodes":[],"marks":[]}}}','{"forward":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"occurrence-rename","to_revision":"occurrence-split","nodes":{"isolated-right":{"before":{"label":"Silence","kind":{"type":"retime","child":"isolated-context","duration":7,"mapping":{"start":5,"end":12},"pitch":"preserve","purpose":"partition"}},"after":{"label":"Silence","kind":{"type":"retime","child":"isolated-context","duration":3,"mapping":{"start":5,"end":8},"pitch":"preserve","purpose":"partition"}}},"isolated-sequence":{"before":null,"after":{"label":"Silence","kind":{"type":"sequence","children":["isolated-right","isolated-tail"]}}},"isolated-tail":{"before":null,"after":{"label":"Silence","kind":{"type":"retime","child":"isolated-tail-context","duration":4,"mapping":{"start":8,"end":12},"pitch":"preserve","purpose":"partition"}}},"isolated-tail-context":{"before":null,"after":{"label":"Independent play","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{"repeat":{"before":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated-right"}],"after":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated-sequence"}]}}},"inverse":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"occurrence-split","to_revision":"occurrence-rename","nodes":{"isolated-right":{"before":{"label":"Silence","kind":{"type":"retime","child":"isolated-context","duration":3,"mapping":{"start":5,"end":8},"pitch":"preserve","purpose":"partition"}},"after":{"label":"Silence","kind":{"type":"retime","child":"isolated-context","duration":7,"mapping":{"start":5,"end":12},"pitch":"preserve","purpose":"partition"}}},"isolated-sequence":{"before":{"label":"Silence","kind":{"type":"sequence","children":["isolated-right","isolated-tail"]}},"after":null},"isolated-tail":{"before":{"label":"Silence","kind":{"type":"retime","child":"isolated-tail-context","duration":4,"mapping":{"start":8,"end":12},"pitch":"preserve","purpose":"partition"}},"after":null},"isolated-tail-context":{"before":{"label":"Independent play","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null}},"assets":{},"marks":{},"overrides":{"repeat":{"before":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated-sequence"}],"after":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated-right"}]}}},"changed_ids":["isolated-right","isolated-sequence","isolated-tail","isolated-tail-context","repeat"],"duration_delta":0,"description":"Edit selected occurrence"}');
INSERT INTO "history" VALUES(6,5,'delete','{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","expected_revision":"occurrence-split","new_revision":"delete","command":{"command":"delete","node":"left"}}','{"forward":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"occurrence-split","to_revision":"delete","nodes":{"14cc5d97-f828-4b9f-8f19-9dd49865253a":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["left","repeat"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}}},"hold":{"before":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"left":{"before":{"label":"Silence","kind":{"type":"retime","child":"hold","duration":5,"mapping":{"start":0,"end":5},"pitch":"preserve","purpose":"partition"}},"after":null}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"987ed9d6-984d-41c7-b681-c2a888e913a9","from_revision":"delete","to_revision":"occurrence-split","nodes":{"14cc5d97-f828-4b9f-8f19-9dd49865253a":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["left","repeat"]}}},"hold":{"before":null,"after":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"left":{"before":null,"after":{"label":"Silence","kind":{"type":"retime","child":"hold","duration":5,"mapping":{"start":0,"end":5},"pitch":"preserve","purpose":"partition"}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["14cc5d97-f828-4b9f-8f19-9dd49865253a","hold","left"],"duration_delta":-5,"description":"Delete beat"}');
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
INSERT INTO "redo" VALUES(1,6);
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('88c97e92-08a2-4548-9a9e-6a69bd7b557c',NULL,'initial','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "88c97e92-08a2-4548-9a9e-6a69bd7b557c",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
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
INSERT INTO "revisions" VALUES('insert','88c97e92-08a2-4548-9a9e-6a69bd7b557c','edit','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "insert",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    },
    "hold": {
      "label": "Silence",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('split','insert','edit','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "split",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "left",
          "right"
        ]
      }
    },
    "hold": {
      "label": "Silence",
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
    "left": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "right-context",
        "duration": 7,
        "mapping": {
          "start": 5,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "right-context": {
      "label": "Silence",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('b2f9e8e6-326b-475e-994b-f920d2066855','split','undo','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "b2f9e8e6-326b-475e-994b-f920d2066855",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
        ]
      }
    },
    "hold": {
      "label": "Silence",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('68db4c7a-70f5-44be-9c4c-058de9dc0f70','b2f9e8e6-326b-475e-994b-f920d2066855','redo','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "68db4c7a-70f5-44be-9c4c-058de9dc0f70",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "left",
          "right"
        ]
      }
    },
    "hold": {
      "label": "Silence",
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
    "left": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "right-context",
        "duration": 7,
        "mapping": {
          "start": 5,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "right-context": {
      "label": "Silence",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('wrap','68db4c7a-70f5-44be-9c4c-058de9dc0f70','edit','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "wrap",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "left",
          "repeat"
        ]
      }
    },
    "hold": {
      "label": "Silence",
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
    "left": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "right",
        "iterations": {
          "runs": [
            {
              "allocation": "wrap",
              "first": 0,
              "count": 3
            }
          ]
        },
        "gap": null
      }
    },
    "right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "right-context",
        "duration": 7,
        "mapping": {
          "start": 5,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "right-context": {
      "label": "Silence",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('occurrence-rename','wrap','edit','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "occurrence-rename",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "left",
          "repeat"
        ]
      }
    },
    "hold": {
      "label": "Silence",
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
    "isolated-context": {
      "label": "Independent play",
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
    "isolated-right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "isolated-context",
        "duration": 7,
        "mapping": {
          "start": 5,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "left": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "right",
        "iterations": {
          "runs": [
            {
              "allocation": "wrap",
              "first": 0,
              "count": 3
            }
          ]
        },
        "gap": null
      }
    },
    "right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "right-context",
        "duration": 7,
        "mapping": {
          "start": 5,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "right-context": {
      "label": "Silence",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {
    "repeat": [
      {
        "iteration": {
          "allocation": "wrap",
          "ordinal": 1
        },
        "root": "isolated-right"
      }
    ]
  }
}
');
INSERT INTO "revisions" VALUES('occurrence-split','occurrence-rename','edit','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "occurrence-split",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "left",
          "repeat"
        ]
      }
    },
    "hold": {
      "label": "Silence",
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
    "isolated-context": {
      "label": "Independent play",
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
    "isolated-right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "isolated-context",
        "duration": 3,
        "mapping": {
          "start": 5,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "isolated-sequence": {
      "label": "Silence",
      "kind": {
        "type": "sequence",
        "children": [
          "isolated-right",
          "isolated-tail"
        ]
      }
    },
    "isolated-tail": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "isolated-tail-context",
        "duration": 4,
        "mapping": {
          "start": 8,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "isolated-tail-context": {
      "label": "Independent play",
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
    "left": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "right",
        "iterations": {
          "runs": [
            {
              "allocation": "wrap",
              "first": 0,
              "count": 3
            }
          ]
        },
        "gap": null
      }
    },
    "right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "right-context",
        "duration": 7,
        "mapping": {
          "start": 5,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "right-context": {
      "label": "Silence",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {
    "repeat": [
      {
        "iteration": {
          "allocation": "wrap",
          "ordinal": 1
        },
        "root": "isolated-sequence"
      }
    ]
  }
}
');
INSERT INTO "revisions" VALUES('delete','occurrence-split','edit','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "delete",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "isolated-context": {
      "label": "Independent play",
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
    "isolated-right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "isolated-context",
        "duration": 3,
        "mapping": {
          "start": 5,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "isolated-sequence": {
      "label": "Silence",
      "kind": {
        "type": "sequence",
        "children": [
          "isolated-right",
          "isolated-tail"
        ]
      }
    },
    "isolated-tail": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "isolated-tail-context",
        "duration": 4,
        "mapping": {
          "start": 8,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "isolated-tail-context": {
      "label": "Independent play",
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
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "right",
        "iterations": {
          "runs": [
            {
              "allocation": "wrap",
              "first": 0,
              "count": 3
            }
          ]
        },
        "gap": null
      }
    },
    "right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "right-context",
        "duration": 7,
        "mapping": {
          "start": 5,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "right-context": {
      "label": "Silence",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {
    "repeat": [
      {
        "iteration": {
          "allocation": "wrap",
          "ordinal": 1
        },
        "root": "isolated-sequence"
      }
    ]
  }
}
');
INSERT INTO "revisions" VALUES('0f138a3a-6e4d-430e-a03d-9fe61127c9ad','delete','undo','{
  "schema_version": 14,
  "project_id": "987ed9d6-984d-41c7-b681-c2a888e913a9",
  "revision_id": "0f138a3a-6e4d-430e-a03d-9fe61127c9ad",
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
  "root": "14cc5d97-f828-4b9f-8f19-9dd49865253a",
  "nodes": {
    "14cc5d97-f828-4b9f-8f19-9dd49865253a": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "left",
          "repeat"
        ]
      }
    },
    "hold": {
      "label": "Silence",
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
    "isolated-context": {
      "label": "Independent play",
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
    "isolated-right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "isolated-context",
        "duration": 3,
        "mapping": {
          "start": 5,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "isolated-sequence": {
      "label": "Silence",
      "kind": {
        "type": "sequence",
        "children": [
          "isolated-right",
          "isolated-tail"
        ]
      }
    },
    "isolated-tail": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "isolated-tail-context",
        "duration": 4,
        "mapping": {
          "start": 8,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "isolated-tail-context": {
      "label": "Independent play",
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
    "left": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "hold",
        "duration": 5,
        "mapping": {
          "start": 0,
          "end": 5
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "right",
        "iterations": {
          "runs": [
            {
              "allocation": "wrap",
              "first": 0,
              "count": 3
            }
          ]
        },
        "gap": null
      }
    },
    "right": {
      "label": "Silence",
      "kind": {
        "type": "retime",
        "child": "right-context",
        "duration": 7,
        "mapping": {
          "start": 5,
          "end": 12
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "right-context": {
      "label": "Silence",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {
    "repeat": [
      {
        "iteration": {
          "allocation": "wrap",
          "ordinal": 1
        },
        "root": "isolated-sequence"
      }
    ]
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
INSERT INTO "state" VALUES(1,'0f138a3a-6e4d-430e-a03d-9fe61127c9ad',5,'generic');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
