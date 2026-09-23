-- Authentic database schema 19 / core schema 13 history.
-- Captured from git revision 789ee07 with a bounded ProjectStore helper on 2026-09-23.
-- Initial multi-binding marks, transparent partition, abandoned rename, partial fragment loss, undo and pending redo.
-- Reopened and validated by the old store, then captured through its SQLite backup API.
-- No external media or local paths.
PRAGMA application_id=1146113585;
PRAGMA user_version=19;
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
INSERT INTO "history" VALUES(1,NULL,'abandoned','{"project_id":"legacy-fragment-history","expected_revision":"initial","new_revision":"abandoned","command":{"command":"rename","node":"second","label":"Abandoned name"}}','{"forward":{"project_id":"legacy-fragment-history","from_revision":"initial","to_revision":"abandoned","nodes":{"second":{"before":{"label":"Second","kind":{"type":"hold","recipe":{"duration":20,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Abandoned name","kind":{"type":"hold","recipe":{"duration":20,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"legacy-fragment-history","from_revision":"abandoned","to_revision":"initial","nodes":{"second":{"before":{"label":"Abandoned name","kind":{"type":"hold","recipe":{"duration":20,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Second","kind":{"type":"hold","recipe":{"duration":20,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["second"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(2,NULL,'rename','{"project_id":"legacy-fragment-history","expected_revision":"undo-abandoned","new_revision":"rename","command":{"command":"rename","node":"second","label":"Kept name"}}','{"forward":{"project_id":"legacy-fragment-history","from_revision":"undo-abandoned","to_revision":"rename","nodes":{"second":{"before":{"label":"Second","kind":{"type":"hold","recipe":{"duration":20,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Kept name","kind":{"type":"hold","recipe":{"duration":20,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"legacy-fragment-history","from_revision":"rename","to_revision":"undo-abandoned","nodes":{"second":{"before":{"label":"Kept name","kind":{"type":"hold","recipe":{"duration":20,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Second","kind":{"type":"hold","recipe":{"duration":20,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["second"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(3,2,'delete','{"project_id":"legacy-fragment-history","expected_revision":"rename","new_revision":"delete","command":{"command":"delete","node":"first"}}','{"forward":{"project_id":"legacy-fragment-history","from_revision":"rename","to_revision":"delete","nodes":{"first":{"before":{"label":"First","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["first","second","partition"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["second","partition"]}}}},"assets":{},"marks":{"kept":{"before":{"owner":"first","label":"Across retained contexts","boundary":{"coordinate":{"space":"local","node":"first","position":{"numerator":"5","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"},"fragments":[{"owner":"second","coordinate":{"space":"local","node":"second","position":{"numerator":"7","denominator":"1"}},"state":{"type":"bound"}},{"owner":"context","coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"3","denominator":"1"}},"state":{"type":"bound"}}]},"after":{"owner":"first","label":"Across retained contexts","boundary":{"coordinate":{"space":"local","node":"first","position":{"numerator":"5","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"unresolved","reason":"owner_missing"},"fragments":[{"owner":"second","coordinate":{"space":"local","node":"second","position":{"numerator":"7","denominator":"1"}},"state":{"type":"bound"}},{"owner":"context","coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"3","denominator":"1"}},"state":{"type":"bound"}}]}},"owned":{"before":{"owner":"first","label":"Across retained contexts","boundary":{"coordinate":{"space":"local","node":"first","position":{"numerator":"5","denominator":"1"}},"bias":"right"},"loss_policy":"delete_owned","state":{"type":"bound"},"fragments":[{"owner":"second","coordinate":{"space":"local","node":"second","position":{"numerator":"7","denominator":"1"}},"state":{"type":"bound"}},{"owner":"context","coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"3","denominator":"1"}},"state":{"type":"bound"}}]},"after":{"owner":"second","label":"Across retained contexts","boundary":{"coordinate":{"space":"local","node":"second","position":{"numerator":"7","denominator":"1"}},"bias":"right"},"loss_policy":"delete_owned","state":{"type":"bound"},"fragments":[{"owner":"context","coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"3","denominator":"1"}},"state":{"type":"bound"}}]}}},"overrides":{}},"inverse":{"project_id":"legacy-fragment-history","from_revision":"delete","to_revision":"rename","nodes":{"first":{"before":null,"after":{"label":"First","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"root":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["second","partition"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["first","second","partition"]}}}},"assets":{},"marks":{"kept":{"before":{"owner":"first","label":"Across retained contexts","boundary":{"coordinate":{"space":"local","node":"first","position":{"numerator":"5","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"unresolved","reason":"owner_missing"},"fragments":[{"owner":"second","coordinate":{"space":"local","node":"second","position":{"numerator":"7","denominator":"1"}},"state":{"type":"bound"}},{"owner":"context","coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"3","denominator":"1"}},"state":{"type":"bound"}}]},"after":{"owner":"first","label":"Across retained contexts","boundary":{"coordinate":{"space":"local","node":"first","position":{"numerator":"5","denominator":"1"}},"bias":"right"},"loss_policy":"keep_unresolved","state":{"type":"bound"},"fragments":[{"owner":"second","coordinate":{"space":"local","node":"second","position":{"numerator":"7","denominator":"1"}},"state":{"type":"bound"}},{"owner":"context","coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"3","denominator":"1"}},"state":{"type":"bound"}}]}},"owned":{"before":{"owner":"second","label":"Across retained contexts","boundary":{"coordinate":{"space":"local","node":"second","position":{"numerator":"7","denominator":"1"}},"bias":"right"},"loss_policy":"delete_owned","state":{"type":"bound"},"fragments":[{"owner":"context","coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"3","denominator":"1"}},"state":{"type":"bound"}}]},"after":{"owner":"first","label":"Across retained contexts","boundary":{"coordinate":{"space":"local","node":"first","position":{"numerator":"5","denominator":"1"}},"bias":"right"},"loss_policy":"delete_owned","state":{"type":"bound"},"fragments":[{"owner":"second","coordinate":{"space":"local","node":"second","position":{"numerator":"7","denominator":"1"}},"state":{"type":"bound"}},{"owner":"context","coordinate":{"space":"occurrence","instance":{"node":"context","repeats":[]},"position":{"numerator":"3","denominator":"1"}},"state":{"type":"bound"}}]}}},"overrides":{}},"changed_ids":["first","root"],"duration_delta":-10,"description":"Delete beat"}');
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
INSERT INTO "redo" VALUES(1,3);
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('initial',NULL,'initial','{
  "schema_version": 13,
  "project_id": "legacy-fragment-history",
  "revision_id": "initial",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 24,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "root",
  "nodes": {
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
    "first": {
      "label": "First",
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
      "label": "Context slice",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 2,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "first",
          "second",
          "partition"
        ]
      }
    },
    "second": {
      "label": "Second",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 20,
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
  "marks": {
    "kept": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "owned": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "single": {
      "owner": "second",
      "label": "Single binding",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "second",
          "position": {
            "numerator": "3",
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
INSERT INTO "revisions" VALUES('abandoned','initial','edit','{
  "schema_version": 13,
  "project_id": "legacy-fragment-history",
  "revision_id": "abandoned",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 24,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "root",
  "nodes": {
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
    "first": {
      "label": "First",
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
      "label": "Context slice",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 2,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "first",
          "second",
          "partition"
        ]
      }
    },
    "second": {
      "label": "Abandoned name",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 20,
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
  "marks": {
    "kept": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "owned": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "single": {
      "owner": "second",
      "label": "Single binding",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "second",
          "position": {
            "numerator": "3",
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
INSERT INTO "revisions" VALUES('undo-abandoned','abandoned','undo','{
  "schema_version": 13,
  "project_id": "legacy-fragment-history",
  "revision_id": "undo-abandoned",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 24,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "root",
  "nodes": {
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
    "first": {
      "label": "First",
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
      "label": "Context slice",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 2,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "first",
          "second",
          "partition"
        ]
      }
    },
    "second": {
      "label": "Second",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 20,
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
  "marks": {
    "kept": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "owned": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "single": {
      "owner": "second",
      "label": "Single binding",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "second",
          "position": {
            "numerator": "3",
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
INSERT INTO "revisions" VALUES('rename','undo-abandoned','edit','{
  "schema_version": 13,
  "project_id": "legacy-fragment-history",
  "revision_id": "rename",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 24,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "root",
  "nodes": {
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
    "first": {
      "label": "First",
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
      "label": "Context slice",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 2,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "first",
          "second",
          "partition"
        ]
      }
    },
    "second": {
      "label": "Kept name",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 20,
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
  "marks": {
    "kept": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "owned": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "single": {
      "owner": "second",
      "label": "Single binding",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "second",
          "position": {
            "numerator": "3",
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
INSERT INTO "revisions" VALUES('delete','rename','edit','{
  "schema_version": 13,
  "project_id": "legacy-fragment-history",
  "revision_id": "delete",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 24,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "root",
  "nodes": {
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
      "label": "Context slice",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 2,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "second",
          "partition"
        ]
      }
    },
    "second": {
      "label": "Kept name",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 20,
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
  "marks": {
    "kept": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "unresolved",
        "reason": "owner_missing"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "owned": {
      "owner": "second",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "second",
          "position": {
            "numerator": "7",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "single": {
      "owner": "second",
      "label": "Single binding",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "second",
          "position": {
            "numerator": "3",
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
INSERT INTO "revisions" VALUES('undo-delete','delete','undo','{
  "schema_version": 13,
  "project_id": "legacy-fragment-history",
  "revision_id": "undo-delete",
  "presentation_basis": {
    "width": 16,
    "height": 16,
    "frame_rate": {
      "numerator": 24,
      "denominator": 1
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": null
  },
  "root": "root",
  "nodes": {
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
    "first": {
      "label": "First",
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
      "label": "Context slice",
      "kind": {
        "type": "retime",
        "child": "context",
        "duration": 6,
        "mapping": {
          "start": 2,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "first",
          "second",
          "partition"
        ]
      }
    },
    "second": {
      "label": "Kept name",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 20,
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
  "marks": {
    "kept": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "keep_unresolved",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "owned": {
      "owner": "first",
      "label": "Across retained contexts",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "first",
          "position": {
            "numerator": "5",
            "denominator": "1"
          }
        },
        "bias": "right"
      },
      "loss_policy": "delete_owned",
      "state": {
        "type": "bound"
      },
      "fragments": [
        {
          "owner": "second",
          "coordinate": {
            "space": "local",
            "node": "second",
            "position": {
              "numerator": "7",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        },
        {
          "owner": "context",
          "coordinate": {
            "space": "occurrence",
            "instance": {
              "node": "context",
              "repeats": []
            },
            "position": {
              "numerator": "3",
              "denominator": "1"
            }
          },
          "state": {
            "type": "bound"
          }
        }
      ]
    },
    "single": {
      "owner": "second",
      "label": "Single binding",
      "boundary": {
        "coordinate": {
          "space": "local",
          "node": "second",
          "position": {
            "numerator": "3",
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
INSERT INTO "state" VALUES(1,'undo-delete',2,'generic');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
