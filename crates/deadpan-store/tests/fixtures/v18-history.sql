-- Authentic database schema 18 / core schema 12 history.
-- Captured from 6420ecd CLI via SQLite backup API on 2026-09-23.
-- Transparent partition, Local/Occurrence marks, abandoned edit, deletion, undo and pending redo.
-- No external media or local paths.
PRAGMA application_id=1146113585;
PRAGMA user_version=18;
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
INSERT INTO "history" VALUES(1,NULL,'insert-partition','{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","expected_revision":"e4d6ae46-1bcf-4659-ab2a-1252a910e521","new_revision":"insert-partition","command":{"command":"insert","parent":"99c44b96-4efe-48fe-ba3a-c7732502f9e8","index":0,"subtree":{"root":"partition","nodes":{"context":{"label":"Retained context","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}},"partition":{"label":"Suffix","kind":{"type":"retime","child":"context","duration":6,"mapping":{"start":4,"end":10},"pitch":"preserve","purpose":"partition"}}},"overrides":{}}}}','{"forward":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"e4d6ae46-1bcf-4659-ab2a-1252a910e521","to_revision":"insert-partition","presentation":{"before":{"basis":{"width":1920,"height":1080,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},"state":{"rate_origin":"provisional","geometry_origin":"default","primary":null}},"after":{"basis":{"width":1920,"height":1080,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},"state":{"rate_origin":"timed_edit","geometry_origin":"default","primary":null}}},"nodes":{"99c44b96-4efe-48fe-ba3a-c7732502f9e8":{"before":{"label":"Sequence","kind":{"type":"sequence","children":[]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["partition"]}}},"context":{"before":null,"after":{"label":"Retained context","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"partition":{"before":null,"after":{"label":"Suffix","kind":{"type":"retime","child":"context","duration":6,"mapping":{"start":4,"end":10},"pitch":"preserve","purpose":"partition"}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"insert-partition","to_revision":"e4d6ae46-1bcf-4659-ab2a-1252a910e521","presentation":{"before":{"basis":{"width":1920,"height":1080,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},"state":{"rate_origin":"timed_edit","geometry_origin":"default","primary":null}},"after":{"basis":{"width":1920,"height":1080,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},"state":{"rate_origin":"provisional","geometry_origin":"default","primary":null}}},"nodes":{"99c44b96-4efe-48fe-ba3a-c7732502f9e8":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["partition"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":[]}}},"context":{"before":{"label":"Retained context","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"partition":{"before":{"label":"Suffix","kind":{"type":"retime","child":"context","duration":6,"mapping":{"start":4,"end":10},"pitch":"preserve","purpose":"partition"}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["99c44b96-4efe-48fe-ba3a-c7732502f9e8","context","partition"],"duration_delta":6,"description":"Insert beats"}');
INSERT INTO "history" VALUES(2,1,'mark-local','{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","expected_revision":"insert-partition","new_revision":"mark-local","command":{"command":"set_mark","id":"cue","owner":"context","label":"Cue","boundary":{"coordinate":{"space":"local","node":"context","position":{"numerator":"7","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved"}}','{"forward":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"insert-partition","to_revision":"mark-local","nodes":{},"assets":{},"marks":{"cue":{"before":null,"after":{"owner":"context","label":"Cue","boundary":{"coordinate":{"space":"local","node":"context","position":{"numerator":"7","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}}}},"overrides":{}},"inverse":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"mark-local","to_revision":"insert-partition","nodes":{},"assets":{},"marks":{"cue":{"before":{"owner":"context","label":"Cue","boundary":{"coordinate":{"space":"local","node":"context","position":{"numerator":"7","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}},"after":null}},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Set mark"}');
INSERT INTO "history" VALUES(3,2,'mark-occurrence','{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","expected_revision":"mark-local","new_revision":"mark-occurrence","command":{"command":"set_mark","id":"edge","owner":"partition","label":"Seam","boundary":{"coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"4","denominator":"1"}},"bias":"left"},"loss_policy":"delete_owned"}}','{"forward":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"mark-local","to_revision":"mark-occurrence","nodes":{},"assets":{},"marks":{"edge":{"before":null,"after":{"owner":"partition","label":"Seam","boundary":{"coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"4","denominator":"1"}},"bias":"left"},"loss_policy":"delete_owned","state":{"type":"bound"}}}},"overrides":{}},"inverse":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"mark-occurrence","to_revision":"mark-local","nodes":{},"assets":{},"marks":{"edge":{"before":{"owner":"partition","label":"Seam","boundary":{"coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"4","denominator":"1"}},"bias":"left"},"loss_policy":"delete_owned","state":{"type":"bound"}},"after":null}},"overrides":{}},"changed_ids":[],"duration_delta":0,"description":"Set mark"}');
INSERT INTO "history" VALUES(4,3,'rename-context','{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","expected_revision":"mark-occurrence","new_revision":"rename-context","command":{"command":"rename","node":"context","label":"Renamed context"}}','{"forward":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"mark-occurrence","to_revision":"rename-context","nodes":{"context":{"before":{"label":"Retained context","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Renamed context","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"rename-context","to_revision":"mark-occurrence","nodes":{"context":{"before":{"label":"Renamed context","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Retained context","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["context"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(5,3,'delete-context','{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","expected_revision":"e7d3181c-4752-408d-817f-a479f570d0de","new_revision":"delete-context","command":{"command":"delete","node":"partition"}}','{"forward":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"e7d3181c-4752-408d-817f-a479f570d0de","to_revision":"delete-context","nodes":{"99c44b96-4efe-48fe-ba3a-c7732502f9e8":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["partition"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":[]}}},"context":{"before":{"label":"Retained context","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"partition":{"before":{"label":"Suffix","kind":{"type":"retime","child":"context","duration":6,"mapping":{"start":4,"end":10},"pitch":"preserve","purpose":"partition"}},"after":null}},"assets":{},"marks":{"cue":{"before":{"owner":"context","label":"Cue","boundary":{"coordinate":{"space":"local","node":"context","position":{"numerator":"7","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}},"after":{"owner":"context","label":"Cue","boundary":{"coordinate":{"space":"local","node":"context","position":{"numerator":"7","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"unresolved","reason":"owner_missing"}}},"edge":{"before":{"owner":"partition","label":"Seam","boundary":{"coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"4","denominator":"1"}},"bias":"left"},"loss_policy":"delete_owned","state":{"type":"bound"}},"after":null}},"overrides":{}},"inverse":{"project_id":"6b85cf61-3e54-4d11-bae0-916ff590ad9d","from_revision":"delete-context","to_revision":"e7d3181c-4752-408d-817f-a479f570d0de","nodes":{"99c44b96-4efe-48fe-ba3a-c7732502f9e8":{"before":{"label":"Sequence","kind":{"type":"sequence","children":[]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["partition"]}}},"context":{"before":null,"after":{"label":"Retained context","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"partition":{"before":null,"after":{"label":"Suffix","kind":{"type":"retime","child":"context","duration":6,"mapping":{"start":4,"end":10},"pitch":"preserve","purpose":"partition"}}}},"assets":{},"marks":{"cue":{"before":{"owner":"context","label":"Cue","boundary":{"coordinate":{"space":"local","node":"context","position":{"numerator":"7","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"unresolved","reason":"owner_missing"}},"after":{"owner":"context","label":"Cue","boundary":{"coordinate":{"space":"local","node":"context","position":{"numerator":"7","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"}}},"edge":{"before":null,"after":{"owner":"partition","label":"Seam","boundary":{"coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"4","denominator":"1"}},"bias":"left"},"loss_policy":"delete_owned","state":{"type":"bound"}}}},"overrides":{}},"changed_ids":["99c44b96-4efe-48fe-ba3a-c7732502f9e8","context","partition"],"duration_delta":-6,"description":"Delete beat"}');
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
INSERT INTO "revisions" VALUES('e4d6ae46-1bcf-4659-ab2a-1252a910e521',NULL,'initial','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "e4d6ae46-1bcf-4659-ab2a-1252a910e521",
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
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
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
INSERT INTO "revisions" VALUES('insert-partition','e4d6ae46-1bcf-4659-ab2a-1252a910e521','edit','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "insert-partition",
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
    "rate_origin": "timed_edit",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "partition"
        ]
      }
    },
    "context": {
      "label": "Retained context",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 10,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "partition": {
      "label": "Suffix",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 4,
          "end": 10
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('mark-local','insert-partition','edit','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "mark-local",
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
    "rate_origin": "timed_edit",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "partition"
        ]
      }
    },
    "context": {
      "label": "Retained context",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 10,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "partition": {
      "label": "Suffix",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 4,
          "end": 10
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    }
  },
  "assets": {},
  "marks": {
    "cue": {
      "owner": "context",
      "label": "Cue",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "context",
          "position": {
            "numerator": "7",
            "denominator": "1"
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
INSERT INTO "revisions" VALUES('mark-occurrence','mark-local','edit','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "mark-occurrence",
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
    "rate_origin": "timed_edit",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "partition"
        ]
      }
    },
    "context": {
      "label": "Retained context",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 10,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "partition": {
      "label": "Suffix",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 4,
          "end": 10
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    }
  },
  "assets": {},
  "marks": {
    "cue": {
      "owner": "context",
      "label": "Cue",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "context",
          "position": {
            "numerator": "7",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "edge": {
      "owner": "partition",
      "label": "Seam",
      "boundary": {
        "coordinate": {
          "space": "occurrence",
          "instance": {
            "node": "context",
            "repeats": []
          },
          "position": {
            "numerator": "4",
            "denominator": "1"
          }
        },
        "bias": "left"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('rename-context','mark-occurrence','edit','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "rename-context",
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
    "rate_origin": "timed_edit",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "partition"
        ]
      }
    },
    "context": {
      "label": "Renamed context",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 10,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "partition": {
      "label": "Suffix",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 4,
          "end": 10
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    }
  },
  "assets": {},
  "marks": {
    "cue": {
      "owner": "context",
      "label": "Cue",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "context",
          "position": {
            "numerator": "7",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "edge": {
      "owner": "partition",
      "label": "Seam",
      "boundary": {
        "coordinate": {
          "space": "occurrence",
          "instance": {
            "node": "context",
            "repeats": []
          },
          "position": {
            "numerator": "4",
            "denominator": "1"
          }
        },
        "bias": "left"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('e7d3181c-4752-408d-817f-a479f570d0de','rename-context','undo','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "e7d3181c-4752-408d-817f-a479f570d0de",
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
    "rate_origin": "timed_edit",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "partition"
        ]
      }
    },
    "context": {
      "label": "Retained context",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 10,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "partition": {
      "label": "Suffix",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 4,
          "end": 10
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    }
  },
  "assets": {},
  "marks": {
    "cue": {
      "owner": "context",
      "label": "Cue",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "context",
          "position": {
            "numerator": "7",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "edge": {
      "owner": "partition",
      "label": "Seam",
      "boundary": {
        "coordinate": {
          "space": "occurrence",
          "instance": {
            "node": "context",
            "repeats": []
          },
          "position": {
            "numerator": "4",
            "denominator": "1"
          }
        },
        "bias": "left"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('delete-context','e7d3181c-4752-408d-817f-a479f570d0de','edit','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "delete-context",
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
    "rate_origin": "timed_edit",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": []
      }
    }
  },
  "assets": {},
  "marks": {
    "cue": {
      "owner": "context",
      "label": "Cue",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "context",
          "position": {
            "numerator": "7",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "unresolved",
        "reason": "owner_missing"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('0d04013a-fb17-4e78-9714-5cbf194b7440','delete-context','undo','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "0d04013a-fb17-4e78-9714-5cbf194b7440",
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
    "rate_origin": "timed_edit",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "partition"
        ]
      }
    },
    "context": {
      "label": "Retained context",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 10,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "partition": {
      "label": "Suffix",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 4,
          "end": 10
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    }
  },
  "assets": {},
  "marks": {
    "cue": {
      "owner": "context",
      "label": "Cue",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "context",
          "position": {
            "numerator": "7",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "edge": {
      "owner": "partition",
      "label": "Seam",
      "boundary": {
        "coordinate": {
          "space": "occurrence",
          "instance": {
            "node": "context",
            "repeats": []
          },
          "position": {
            "numerator": "4",
            "denominator": "1"
          }
        },
        "bias": "left"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('532306a6-edc5-414b-baaa-ab64c9528b2c','0d04013a-fb17-4e78-9714-5cbf194b7440','redo','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "532306a6-edc5-414b-baaa-ab64c9528b2c",
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
    "rate_origin": "timed_edit",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": []
      }
    }
  },
  "assets": {},
  "marks": {
    "cue": {
      "owner": "context",
      "label": "Cue",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "context",
          "position": {
            "numerator": "7",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "unresolved",
        "reason": "owner_missing"
      }
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('2c05ad2d-b9b3-431a-8e9e-6432233e4593','532306a6-edc5-414b-baaa-ab64c9528b2c','undo','{
  "schema_version": 12,
  "project_id": "6b85cf61-3e54-4d11-bae0-916ff590ad9d",
  "revision_id": "2c05ad2d-b9b3-431a-8e9e-6432233e4593",
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
    "rate_origin": "timed_edit",
    "geometry_origin": "default",
    "primary": null
  },
  "root": "99c44b96-4efe-48fe-ba3a-c7732502f9e8",
  "nodes": {
    "99c44b96-4efe-48fe-ba3a-c7732502f9e8": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "partition"
        ]
      }
    },
    "context": {
      "label": "Retained context",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 10,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "partition": {
      "label": "Suffix",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 4,
          "end": 10
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    }
  },
  "assets": {},
  "marks": {
    "cue": {
      "owner": "context",
      "label": "Cue",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "context",
          "position": {
            "numerator": "7",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      }
    },
    "edge": {
      "owner": "partition",
      "label": "Seam",
      "boundary": {
        "coordinate": {
          "space": "occurrence",
          "instance": {
            "node": "context",
            "repeats": []
          },
          "position": {
            "numerator": "4",
            "denominator": "1"
          }
        },
        "bias": "left"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      }
    }
  },
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
INSERT INTO "state" VALUES(1,'2c05ad2d-b9b3-431a-8e9e-6432233e4593',3,'generic');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
