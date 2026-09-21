-- Generated with Deadpan 2cb80633b9ed9458ccd0f36ffbf355187dc9bb49 (database schema 5).
-- Includes current, stale and detached requests, monotonic clocks, and pending redo.
BEGIN TRANSACTION;
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
INSERT INTO "generation_requests" VALUES('first','project','hold',1,'setup-2','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','{"video":{"frames":12,"frame_rate":{"numerator":30000,"denominator":1001},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":12}','stale');
INSERT INTO "generation_requests" VALUES('current','project','hold',2,'setup-2','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','{"video":{"frames":12,"frame_rate":{"numerator":30000,"denominator":1001},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":12}','current');
INSERT INTO "generation_requests" VALUES('deleted-request','project','deleted',1,'setup-2','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb','{"video":{"frames":18,"frame_rate":{"numerator":30000,"denominator":1001},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":18}','detached');
INSERT INTO "generation_requests" VALUES('changed-request','project','changed',1,'setup-2','cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc','{"video":{"frames":24,"frame_rate":{"numerator":30000,"denominator":1001},"width":512,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"pack","pack_version":"v1","runtime_id":"runtime","runtime_version":"v1","seed":24}','stale');
CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
INSERT INTO "history" VALUES(1,NULL,'delete','{"project_id":"project","expected_revision":"setup-2","new_revision":"delete","command":{"command":"delete","node":"deleted"}}','{"forward":{"project_id":"project","from_revision":"setup-2","to_revision":"delete","nodes":{"deleted":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":18,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold","deleted","changed"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold","changed"]}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project","from_revision":"delete","to_revision":"setup-2","nodes":{"deleted":{"before":null,"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":18,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold","changed"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold","deleted","changed"]}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["deleted","root"],"duration_delta":-18,"description":"Delete beat"}');
INSERT INTO "history" VALUES(2,1,'resize','{"project_id":"project","expected_revision":"delete","new_revision":"resize","command":{"command":"set_hold_duration","node":"changed","duration":25}}','{"forward":{"project_id":"project","from_revision":"delete","to_revision":"resize","nodes":{"changed":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":24,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":25,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"project","from_revision":"resize","to_revision":"delete","nodes":{"changed":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":25,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":24,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["changed"],"duration_delta":1,"description":"Change hold duration"}');
CREATE TABLE hold_request_clocks (
    hold_id TEXT PRIMARY KEY,
    high_water INTEGER NOT NULL
        CHECK (high_water BETWEEN 1 AND 9223372036854775807)
) STRICT;
INSERT INTO "hold_request_clocks" VALUES('hold',2);
INSERT INTO "hold_request_clocks" VALUES('deleted',1);
INSERT INTO "hold_request_clocks" VALUES('changed',1);
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
INSERT INTO "revisions" VALUES('setup-2',NULL,'initial','{
  "schema_version": 4,
  "project_id": "project",
  "revision_id": "setup-2",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "changed": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 24,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
    "deleted": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 18,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
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
          "hold",
          "deleted",
          "changed"
        ]
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('delete','setup-2','edit','{
  "schema_version": 4,
  "project_id": "project",
  "revision_id": "delete",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "changed": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 24,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
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
          "hold",
          "changed"
        ]
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('resize','delete','edit','{
  "schema_version": 4,
  "project_id": "project",
  "revision_id": "resize",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "changed": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 25,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
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
          "hold",
          "changed"
        ]
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('undo-resize','resize','undo','{
  "schema_version": 4,
  "project_id": "project",
  "revision_id": "undo-resize",
  "presentation_basis": {
    "width": 1920,
    "height": 1080,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "root",
  "nodes": {
    "changed": {
      "label": "Pause",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 24,
          "video": {
            "type": "background"
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    },
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
          "hold",
          "changed"
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
INSERT INTO "state" VALUES(1,'undo-resize',1);
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
COMMIT;
PRAGMA application_id=1146113585;
PRAGMA user_version=5;
