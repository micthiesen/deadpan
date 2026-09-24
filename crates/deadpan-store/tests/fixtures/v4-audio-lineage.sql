-- Authentic database schema 4 / core schema 4 occurrence-copy history.
-- Captured on 2026-09-23 with the preserved CLI from revision
-- 8b79526ca3ff9bca633aa9e9d795bcc9a0c0c2d1.
-- CLI SHA-256: 0bfe374751c8737fa4697cf7b638cd5580410524b4372c4b66aee9efed67bb58.
-- Repeat occurrence isolation, undo/redo, clear override and pending redo.
-- Validated by the old executable and captured through SQLite backup.
-- No external media or local paths.
PRAGMA application_id=1146113585;
PRAGMA user_version=4;
BEGIN TRANSACTION;
CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
INSERT INTO "history" VALUES(1,NULL,'insert','{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","expected_revision":"02967890-6dc4-4db5-98bd-c92c97171d1c","new_revision":"insert","command":{"command":"insert","parent":"d2ab4e96-f160-4886-9884-82cf90312714","index":0,"subtree":{"root":"hold","nodes":{"hold":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"overrides":{}}}}','{"forward":{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","from_revision":"02967890-6dc4-4db5-98bd-c92c97171d1c","to_revision":"insert","nodes":{"d2ab4e96-f160-4886-9884-82cf90312714":{"before":{"label":"Sequence","kind":{"type":"sequence","children":[]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}}},"hold":{"before":null,"after":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","from_revision":"insert","to_revision":"02967890-6dc4-4db5-98bd-c92c97171d1c","nodes":{"d2ab4e96-f160-4886-9884-82cf90312714":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":[]}}},"hold":{"before":{"label":"Silence","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["d2ab4e96-f160-4886-9884-82cf90312714","hold"],"duration_delta":12,"description":"Insert beats"}');
INSERT INTO "history" VALUES(2,1,'wrap','{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","expected_revision":"insert","new_revision":"wrap","command":{"command":"wrap_repeat","node":"hold","id":"repeat","plays":3,"gap":null,"anchor_policy":"first"}}','{"forward":{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","from_revision":"insert","to_revision":"wrap","nodes":{"d2ab4e96-f160-4886-9884-82cf90312714":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}}},"repeat":{"before":null,"after":{"label":"Repeat","kind":{"type":"repeat","child":"hold","iterations":{"runs":[{"allocation":"wrap","first":0,"count":3}]},"gap":null}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","from_revision":"wrap","to_revision":"insert","nodes":{"d2ab4e96-f160-4886-9884-82cf90312714":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}}},"repeat":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"hold","iterations":{"runs":[{"allocation":"wrap","first":0,"count":3}]},"gap":null}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["d2ab4e96-f160-4886-9884-82cf90312714","repeat"],"duration_delta":24,"description":"Wrap repeat"}');
INSERT INTO "history" VALUES(3,2,'occurrence-copy','{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","expected_revision":"wrap","new_revision":"occurrence-copy","command":{"command":"edit_occurrence","instance":{"node":"hold","repeats":[{"node":"repeat","iteration":{"allocation":"wrap","ordinal":1}}]},"edit":{"type":"rename","label":"Independent play"},"identities":{"nodes":["isolated"],"marks":[]}}}','{"forward":{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","from_revision":"wrap","to_revision":"occurrence-copy","nodes":{"isolated":{"before":null,"after":{"label":"Independent play","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{"repeat":{"before":null,"after":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated"}]}}},"inverse":{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","from_revision":"occurrence-copy","to_revision":"wrap","nodes":{"isolated":{"before":{"label":"Independent play","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null}},"assets":{},"marks":{},"overrides":{"repeat":{"before":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated"}],"after":null}}},"changed_ids":["isolated","repeat"],"duration_delta":0,"description":"Edit selected occurrence"}');
INSERT INTO "history" VALUES(4,3,'clear','{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","expected_revision":"0abc3b08-8e57-4b9a-8a33-27c5fa875b8a","new_revision":"clear","command":{"command":"clear_play_override","node":"repeat","iteration":{"allocation":"wrap","ordinal":1}}}','{"forward":{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","from_revision":"0abc3b08-8e57-4b9a-8a33-27c5fa875b8a","to_revision":"clear","nodes":{"isolated":{"before":{"label":"Independent play","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null}},"assets":{},"marks":{},"overrides":{"repeat":{"before":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated"}],"after":null}}},"inverse":{"project_id":"78c5cfac-e572-4bf3-93bb-d638376ff91d","from_revision":"clear","to_revision":"0abc3b08-8e57-4b9a-8a33-27c5fa875b8a","nodes":{"isolated":{"before":null,"after":{"label":"Independent play","kind":{"type":"hold","recipe":{"duration":12,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{"repeat":{"before":null,"after":[{"iteration":{"allocation":"wrap","ordinal":1},"root":"isolated"}]}}},"changed_ids":["isolated","repeat"],"duration_delta":0,"description":"Clear play override"}');
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
INSERT INTO "revisions" VALUES('02967890-6dc4-4db5-98bd-c92c97171d1c',NULL,'initial','{
  "schema_version": 4,
  "project_id": "78c5cfac-e572-4bf3-93bb-d638376ff91d",
  "revision_id": "02967890-6dc4-4db5-98bd-c92c97171d1c",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "d2ab4e96-f160-4886-9884-82cf90312714",
  "nodes": {
    "d2ab4e96-f160-4886-9884-82cf90312714": {
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
INSERT INTO "revisions" VALUES('insert','02967890-6dc4-4db5-98bd-c92c97171d1c','edit','{
  "schema_version": 4,
  "project_id": "78c5cfac-e572-4bf3-93bb-d638376ff91d",
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
  "root": "d2ab4e96-f160-4886-9884-82cf90312714",
  "nodes": {
    "d2ab4e96-f160-4886-9884-82cf90312714": {
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
INSERT INTO "revisions" VALUES('wrap','insert','edit','{
  "schema_version": 4,
  "project_id": "78c5cfac-e572-4bf3-93bb-d638376ff91d",
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
  "root": "d2ab4e96-f160-4886-9884-82cf90312714",
  "nodes": {
    "d2ab4e96-f160-4886-9884-82cf90312714": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
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
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "hold",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('occurrence-copy','wrap','edit','{
  "schema_version": 4,
  "project_id": "78c5cfac-e572-4bf3-93bb-d638376ff91d",
  "revision_id": "occurrence-copy",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "d2ab4e96-f160-4886-9884-82cf90312714",
  "nodes": {
    "d2ab4e96-f160-4886-9884-82cf90312714": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
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
    "isolated": {
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
        "child": "hold",
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
        "root": "isolated"
      }
    ]
  }
}
');
INSERT INTO "revisions" VALUES('ab5b14c9-622c-427a-bdb8-efea4dcdfb81','occurrence-copy','undo','{
  "schema_version": 4,
  "project_id": "78c5cfac-e572-4bf3-93bb-d638376ff91d",
  "revision_id": "ab5b14c9-622c-427a-bdb8-efea4dcdfb81",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "d2ab4e96-f160-4886-9884-82cf90312714",
  "nodes": {
    "d2ab4e96-f160-4886-9884-82cf90312714": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
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
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "hold",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('0abc3b08-8e57-4b9a-8a33-27c5fa875b8a','ab5b14c9-622c-427a-bdb8-efea4dcdfb81','redo','{
  "schema_version": 4,
  "project_id": "78c5cfac-e572-4bf3-93bb-d638376ff91d",
  "revision_id": "0abc3b08-8e57-4b9a-8a33-27c5fa875b8a",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "d2ab4e96-f160-4886-9884-82cf90312714",
  "nodes": {
    "d2ab4e96-f160-4886-9884-82cf90312714": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
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
    "isolated": {
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
        "child": "hold",
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
        "root": "isolated"
      }
    ]
  }
}
');
INSERT INTO "revisions" VALUES('clear','0abc3b08-8e57-4b9a-8a33-27c5fa875b8a','edit','{
  "schema_version": 4,
  "project_id": "78c5cfac-e572-4bf3-93bb-d638376ff91d",
  "revision_id": "clear",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "d2ab4e96-f160-4886-9884-82cf90312714",
  "nodes": {
    "d2ab4e96-f160-4886-9884-82cf90312714": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
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
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "hold",
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
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('e9a66a4a-bf56-40bb-9ca2-a6d0e3bfc6dd','clear','undo','{
  "schema_version": 4,
  "project_id": "78c5cfac-e572-4bf3-93bb-d638376ff91d",
  "revision_id": "e9a66a4a-bf56-40bb-9ca2-a6d0e3bfc6dd",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "d2ab4e96-f160-4886-9884-82cf90312714",
  "nodes": {
    "d2ab4e96-f160-4886-9884-82cf90312714": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
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
    "isolated": {
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
        "child": "hold",
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
        "root": "isolated"
      }
    ]
  }
}
');
CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id)
        ) STRICT;
INSERT INTO "state" VALUES(1,'e9a66a4a-bf56-40bb-9ca2-a6d0e3bfc6dd',3);
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
COMMIT;
