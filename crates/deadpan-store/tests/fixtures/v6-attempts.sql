-- Generated with Deadpan 5100432be7b95600d88179254624dd9d78096714 (database schema 6, core schema 4).
-- Real old-binary fixture: pending redo, nine current requests, ten attempts, selected and evicted receipts.
-- Includes worker/host failure, cancellation, running, queued, loading and validating states.
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
INSERT INTO "generation_attempt_heads" VALUES('request-0',2,'selected','selected');
INSERT INTO "generation_attempt_heads" VALUES('request-1',1,'attempt-1',NULL);
INSERT INTO "generation_attempt_heads" VALUES('request-2',1,'attempt-2',NULL);
INSERT INTO "generation_attempt_heads" VALUES('request-3',1,'attempt-3',NULL);
INSERT INTO "generation_attempt_heads" VALUES('request-4',1,'attempt-4',NULL);
INSERT INTO "generation_attempt_heads" VALUES('request-5',1,'attempt-5',NULL);
INSERT INTO "generation_attempt_heads" VALUES('request-6',1,'attempt-6',NULL);
INSERT INTO "generation_attempt_heads" VALUES('request-7',1,'attempt-7',NULL);
INSERT INTO "generation_attempt_heads" VALUES('request-8',1,'attempt-8',NULL);
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
INSERT INTO "generation_attempts" VALUES('request-0','attempt-0',1,'cancel-attempt-0','ready','inference',6,NULL,'{"media":{"reference":"outputs/candidate.mp4","sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","byte_length":8192},"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"provider":{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":0}}',NULL,NULL,NULL);
INSERT INTO "generation_attempts" VALUES('request-0','selected',2,'cancel-selected','ready','inference',6,NULL,'{"media":{"reference":"outputs/candidate.mp4","sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","byte_length":8192},"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"provider":{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":0}}',NULL,NULL,NULL);
INSERT INTO "generation_attempts" VALUES('request-1','attempt-1',1,'cancel-attempt-1','failed',NULL,2,NULL,NULL,'worker','internal','fixture worker failure');
INSERT INTO "generation_attempts" VALUES('request-2','attempt-2',1,'cancel-attempt-2','failed',NULL,2,NULL,NULL,'host','output_validation_failed','fixture host failure');
INSERT INTO "generation_attempts" VALUES('request-3','attempt-3',1,'cancel-attempt-3','cancelled','inference',6,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "generation_attempts" VALUES('request-4','attempt-4',1,'cancel-attempt-4','running','inference',4,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "generation_attempts" VALUES('request-5','attempt-5',1,'cancel-attempt-5','cancelling','inference',5,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "generation_attempts" VALUES('request-6','attempt-6',1,'cancel-attempt-6','queued',NULL,1,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "generation_attempts" VALUES('request-7','attempt-7',1,'cancel-attempt-7','loading','model_loading',3,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "generation_attempts" VALUES('request-8','attempt-8',1,'cancel-attempt-8','validating','inference',5,NULL,'{"media":{"reference":"outputs/candidate.mp4","sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","byte_length":8192},"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"provider":{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":8}}',NULL,NULL,NULL);
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
INSERT INTO "generation_candidate_receipts" VALUES('request-0','attempt-0','Candidates/request-0/old.mp4','eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',8192,'{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":0}','deadpan-media','1.0','evicted');
INSERT INTO "generation_candidate_receipts" VALUES('request-0','selected','Candidates/request-0/selected.mp4','ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff',8192,'{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":0}','deadpan-media','1.0','present');
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
INSERT INTO "generation_requests" VALUES('request-0','project','hold-0',1,'undo-rename','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":0}','current');
INSERT INTO "generation_requests" VALUES('request-1','project','hold-1',1,'undo-rename','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":1}','current');
INSERT INTO "generation_requests" VALUES('request-2','project','hold-2',1,'undo-rename','cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":2}','current');
INSERT INTO "generation_requests" VALUES('request-3','project','hold-3',1,'undo-rename','dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":3}','current');
INSERT INTO "generation_requests" VALUES('request-4','project','hold-4',1,'undo-rename','eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":4}','current');
INSERT INTO "generation_requests" VALUES('request-5','project','hold-5',1,'undo-rename','ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":5}','current');
INSERT INTO "generation_requests" VALUES('request-6','project','hold-6',1,'undo-rename','0000000000000000000000000000000000000000000000000000000000000000','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":6}','current');
INSERT INTO "generation_requests" VALUES('request-7','project','hold-7',1,'undo-rename','1111111111111111111111111111111111111111111111111111111111111111','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":7}','current');
INSERT INTO "generation_requests" VALUES('request-8','project','hold-8',1,'undo-rename','2222222222222222222222222222222222222222222222222222222222222222','{"video":{"frames":12,"frame_rate":{"numerator":30,"denominator":1},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":8}','current');
CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
INSERT INTO "history" VALUES(1,NULL,'rename','{"project_id":"project","expected_revision":"setup-8","new_revision":"rename","command":{"command":"rename","node":"hold-0","label":"Renamed"}}','{"forward":{"project_id":"project","from_revision":"setup-8","to_revision":"rename","nodes":{"hold-0":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Renamed","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project","from_revision":"rename","to_revision":"setup-8","nodes":{"hold-0":{"before":{"label":"Renamed","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["hold-0"],"duration_delta":0,"description":"Rename beat"}');
CREATE TABLE hold_request_clocks (
    hold_id TEXT PRIMARY KEY,
    high_water INTEGER NOT NULL
        CHECK (high_water BETWEEN 1 AND 9223372036854775807)
) STRICT;
INSERT INTO "hold_request_clocks" VALUES('hold-0',1);
INSERT INTO "hold_request_clocks" VALUES('hold-1',1);
INSERT INTO "hold_request_clocks" VALUES('hold-2',1);
INSERT INTO "hold_request_clocks" VALUES('hold-3',1);
INSERT INTO "hold_request_clocks" VALUES('hold-4',1);
INSERT INTO "hold_request_clocks" VALUES('hold-5',1);
INSERT INTO "hold_request_clocks" VALUES('hold-6',1);
INSERT INTO "hold_request_clocks" VALUES('hold-7',1);
INSERT INTO "hold_request_clocks" VALUES('hold-8',1);
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
INSERT INTO "revisions" VALUES('setup-8',NULL,'initial','{
  "schema_version": 4,
  "project_id": "project",
  "revision_id": "setup-8",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
    "frame_rate": {
      "numerator": 30,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "hold-0": {
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
    "hold-1": {
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
    "hold-2": {
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
    "hold-3": {
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
    "hold-4": {
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
    "hold-5": {
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
    "hold-6": {
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
    "hold-7": {
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
    "hold-8": {
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
          "hold-0",
          "hold-1",
          "hold-2",
          "hold-3",
          "hold-4",
          "hold-5",
          "hold-6",
          "hold-7",
          "hold-8"
        ]
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('rename','setup-8','edit','{
  "schema_version": 4,
  "project_id": "project",
  "revision_id": "rename",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
    "frame_rate": {
      "numerator": 30,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "hold-0": {
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
    "hold-1": {
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
    "hold-2": {
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
    "hold-3": {
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
    "hold-4": {
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
    "hold-5": {
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
    "hold-6": {
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
    "hold-7": {
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
    "hold-8": {
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
          "hold-0",
          "hold-1",
          "hold-2",
          "hold-3",
          "hold-4",
          "hold-5",
          "hold-6",
          "hold-7",
          "hold-8"
        ]
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('undo-rename','rename','undo','{
  "schema_version": 4,
  "project_id": "project",
  "revision_id": "undo-rename",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
    "frame_rate": {
      "numerator": 30,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "hold-0": {
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
    "hold-1": {
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
    "hold-2": {
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
    "hold-3": {
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
    "hold-4": {
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
    "hold-5": {
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
    "hold-6": {
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
    "hold-7": {
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
    "hold-8": {
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
          "hold-0",
          "hold-1",
          "hold-2",
          "hold-3",
          "hold-4",
          "hold-5",
          "hold-6",
          "hold-7",
          "hold-8"
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
INSERT INTO "state" VALUES(1,'undo-rename',NULL);
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
PRAGMA application_id=1146113585;
PRAGMA user_version=6;
