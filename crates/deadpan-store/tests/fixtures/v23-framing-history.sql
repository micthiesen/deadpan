PRAGMA application_id=1146113585;
PRAGMA user_version=23;
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
INSERT INTO "history" VALUES(1,NULL,'pause-one','{"project_id":"fae28ee2-efb0-4435-a95d-e99a4f7081cb","expected_revision":"1b4455a7-d09c-445e-9f8a-4ff7e9da61ac","new_revision":"pause-one","command":{"command":"insert_time","at":0,"hold":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}},"id":"hold","identities":{"nodes":["pause-one-copy-0","pause-one-copy-1","pause-one-copy-2","pause-one-copy-3","pause-one-copy-4"]},"timing":{"allocation":"pause-one","ordinal":0}}}','{"forward":{"project_id":"fae28ee2-efb0-4435-a95d-e99a4f7081cb","from_revision":"1b4455a7-d09c-445e-9f8a-4ff7e9da61ac","to_revision":"pause-one","nodes":{"c289aced-ac0a-4d77-8bc0-121f54591304":{"before":{"label":"Sequence","kind":{"type":"sequence","children":[]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}}},"hold":{"before":null,"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"fae28ee2-efb0-4435-a95d-e99a4f7081cb","from_revision":"pause-one","to_revision":"1b4455a7-d09c-445e-9f8a-4ff7e9da61ac","nodes":{"c289aced-ac0a-4d77-8bc0-121f54591304":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":[]}}},"hold":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["c289aced-ac0a-4d77-8bc0-121f54591304","hold"],"duration_delta":8,"description":"Insert pause"}');
INSERT INTO "history" VALUES(2,1,'pause-two','{"project_id":"fae28ee2-efb0-4435-a95d-e99a4f7081cb","expected_revision":"pause-one","new_revision":"pause-two","command":{"command":"insert_time","at":3,"hold":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}},"id":"inserted","identities":{"nodes":["pause-two-copy-0","pause-two-copy-1","pause-two-copy-2","pause-two-copy-3","pause-two-copy-4","pause-two-copy-5"]},"timing":{"allocation":"pause-two","ordinal":0}}}','{"forward":{"project_id":"fae28ee2-efb0-4435-a95d-e99a4f7081cb","from_revision":"pause-one","to_revision":"pause-two","nodes":{"c289aced-ac0a-4d77-8bc0-121f54591304":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["pause-two-copy-0","inserted","pause-two-copy-1"]}}},"inserted":{"before":null,"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}}},"pause-two-copy-0":{"before":null,"after":{"label":"Pause","kind":{"type":"retime","child":"hold","duration":3,"mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}}},"pause-two-copy-1":{"before":null,"after":{"label":"Pause","kind":{"type":"retime","child":"pause-two-copy-2","duration":5,"mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}}},"pause-two-copy-2":{"before":null,"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{},"audio_lineage":{"hold":{"before":null,"after":{"allocation":"pause-two","origin":"hold"}},"pause-two-copy-2":{"before":null,"after":{"allocation":"pause-two","origin":"hold"}}},"audio_bindings":{"before":{"timings":[],"bindings":{}},"after":{"timings":[{"id":{"allocation":"pause-two","ordinal":0},"layout":{"root":"c289aced-ac0a-4d77-8bc0-121f54591304","rate":{"numerator":30000,"denominator":1001},"nodes":{"c289aced-ac0a-4d77-8bc0-121f54591304":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["hold"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{}}}],"bindings":{"hold":{"lattice":{"reference":{"timing":{"allocation":"pause-two","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":null},"pause-two-copy-2":{"lattice":{"reference":{"timing":{"allocation":"pause-two","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"3","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"pause-two","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"3","denominator":"1"}}]}}}}}}},"inverse":{"project_id":"fae28ee2-efb0-4435-a95d-e99a4f7081cb","from_revision":"pause-two","to_revision":"pause-one","nodes":{"c289aced-ac0a-4d77-8bc0-121f54591304":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["pause-two-copy-0","inserted","pause-two-copy-1"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["hold"]}}},"inserted":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null},"pause-two-copy-0":{"before":{"label":"Pause","kind":{"type":"retime","child":"hold","duration":3,"mapping":{"start":0,"end":3},"pitch":"preserve","purpose":"partition"}},"after":null},"pause-two-copy-1":{"before":{"label":"Pause","kind":{"type":"retime","child":"pause-two-copy-2","duration":5,"mapping":{"start":3,"end":8},"pitch":"preserve","purpose":"partition"}},"after":null},"pause-two-copy-2":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":8,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":null}},"assets":{},"marks":{},"overrides":{},"audio_lineage":{"hold":{"before":{"allocation":"pause-two","origin":"hold"},"after":null},"pause-two-copy-2":{"before":{"allocation":"pause-two","origin":"hold"},"after":null}},"audio_bindings":{"before":{"timings":[{"id":{"allocation":"pause-two","ordinal":0},"layout":{"root":"c289aced-ac0a-4d77-8bc0-121f54591304","rate":{"numerator":30000,"denominator":1001},"nodes":{"c289aced-ac0a-4d77-8bc0-121f54591304":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"sequence","children":["hold"]}},"hold":{"duration":8,"edges":{"node_start":"automatic","node_end":"automatic","source_placement_start":"automatic","source_placement_end":"automatic","repeat_gap_start":"automatic","repeat_gap_end":"automatic"},"kind":{"type":"hold","audio":{"type":"silence"}}}},"overrides":{}}}],"bindings":{"hold":{"lattice":{"reference":{"timing":{"allocation":"pause-two","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":null},"pause-two-copy-2":{"lattice":{"reference":{"timing":{"allocation":"pause-two","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"resume":{"local_boundary":{"numerator":"3","denominator":"1"},"phase":{"constant":{"numerator":"0","denominator":"1"},"terms":[{"placement":{"reference":{"timing":{"allocation":"pause-two","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold"},"arguments":[],"births":[]},"from_local":{"numerator":"0","denominator":"1"},"to_local":{"numerator":"3","denominator":"1"}}]}}}}},"after":{"timings":[],"bindings":{}}}},"changed_ids":["c289aced-ac0a-4d77-8bc0-121f54591304","hold","inserted","pause-two-copy-0","pause-two-copy-1","pause-two-copy-2"],"duration_delta":2,"description":"Insert pause"}');
INSERT INTO "history" VALUES(3,2,'rename','{"project_id":"fae28ee2-efb0-4435-a95d-e99a4f7081cb","expected_revision":"pause-two","new_revision":"rename","command":{"command":"rename","node":"inserted","label":"Old binary pause"}}','{"forward":{"project_id":"fae28ee2-efb0-4435-a95d-e99a4f7081cb","from_revision":"pause-two","to_revision":"rename","nodes":{"inserted":{"before":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Old binary pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"fae28ee2-efb0-4435-a95d-e99a4f7081cb","from_revision":"rename","to_revision":"pause-two","nodes":{"inserted":{"before":{"label":"Old binary pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Pause","kind":{"type":"hold","recipe":{"duration":2,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["inserted"],"duration_delta":0,"description":"Rename beat"}');
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
INSERT INTO "revisions" VALUES('1b4455a7-d09c-445e-9f8a-4ff7e9da61ac',NULL,'initial','{
  "schema_version": 17,
  "project_id": "fae28ee2-efb0-4435-a95d-e99a4f7081cb",
  "revision_id": "1b4455a7-d09c-445e-9f8a-4ff7e9da61ac",
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
  "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
  "nodes": {
    "c289aced-ac0a-4d77-8bc0-121f54591304": {
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
INSERT INTO "revisions" VALUES('pause-one','1b4455a7-d09c-445e-9f8a-4ff7e9da61ac','edit','{
  "schema_version": 17,
  "project_id": "fae28ee2-efb0-4435-a95d-e99a4f7081cb",
  "revision_id": "pause-one",
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
  "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
  "nodes": {
    "c289aced-ac0a-4d77-8bc0-121f54591304": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold"
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
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('pause-two','pause-one','edit','{
  "schema_version": 17,
  "project_id": "fae28ee2-efb0-4435-a95d-e99a4f7081cb",
  "revision_id": "pause-two",
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
  "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
  "nodes": {
    "c289aced-ac0a-4d77-8bc0-121f54591304": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "pause-two-copy-0",
          "inserted",
          "pause-two-copy-1"
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
      }
    },
    "inserted": {
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
    "pause-two-copy-0": {
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
    "pause-two-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "pause-two-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "pause-two-copy-2": {
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
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "pause-two",
      "origin": "hold"
    },
    "pause-two-copy-2": {
      "allocation": "pause-two",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "pause-two",
          "ordinal": 0
        },
        "layout": {
          "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "c289aced-ac0a-4d77-8bc0-121f54591304": {
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
              "allocation": "pause-two",
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
        "resume": null
      },
      "pause-two-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "pause-two",
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
                      "allocation": "pause-two",
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
        }
      }
    }
  }
}
');
INSERT INTO "revisions" VALUES('rename','pause-two','edit','{
  "schema_version": 17,
  "project_id": "fae28ee2-efb0-4435-a95d-e99a4f7081cb",
  "revision_id": "rename",
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
  "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
  "nodes": {
    "c289aced-ac0a-4d77-8bc0-121f54591304": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "pause-two-copy-0",
          "inserted",
          "pause-two-copy-1"
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
      }
    },
    "inserted": {
      "label": "Old binary pause",
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
    "pause-two-copy-0": {
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
    "pause-two-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "pause-two-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "pause-two-copy-2": {
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
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "pause-two",
      "origin": "hold"
    },
    "pause-two-copy-2": {
      "allocation": "pause-two",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "pause-two",
          "ordinal": 0
        },
        "layout": {
          "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "c289aced-ac0a-4d77-8bc0-121f54591304": {
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
              "allocation": "pause-two",
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
        "resume": null
      },
      "pause-two-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "pause-two",
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
                      "allocation": "pause-two",
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
        }
      }
    }
  }
}
');
INSERT INTO "revisions" VALUES('0be4a513-c0fd-4ae6-bfcc-ca952045cfd6','rename','undo','{
  "schema_version": 17,
  "project_id": "fae28ee2-efb0-4435-a95d-e99a4f7081cb",
  "revision_id": "0be4a513-c0fd-4ae6-bfcc-ca952045cfd6",
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
  "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
  "nodes": {
    "c289aced-ac0a-4d77-8bc0-121f54591304": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "pause-two-copy-0",
          "inserted",
          "pause-two-copy-1"
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
      }
    },
    "inserted": {
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
    "pause-two-copy-0": {
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
    "pause-two-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "pause-two-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "pause-two-copy-2": {
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
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "pause-two",
      "origin": "hold"
    },
    "pause-two-copy-2": {
      "allocation": "pause-two",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "pause-two",
          "ordinal": 0
        },
        "layout": {
          "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "c289aced-ac0a-4d77-8bc0-121f54591304": {
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
              "allocation": "pause-two",
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
        "resume": null
      },
      "pause-two-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "pause-two",
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
                      "allocation": "pause-two",
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
        }
      }
    }
  }
}
');
INSERT INTO "revisions" VALUES('fc69a81a-12c7-4899-bebe-306ca7cebd4f','0be4a513-c0fd-4ae6-bfcc-ca952045cfd6','redo','{
  "schema_version": 17,
  "project_id": "fae28ee2-efb0-4435-a95d-e99a4f7081cb",
  "revision_id": "fc69a81a-12c7-4899-bebe-306ca7cebd4f",
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
  "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
  "nodes": {
    "c289aced-ac0a-4d77-8bc0-121f54591304": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "pause-two-copy-0",
          "inserted",
          "pause-two-copy-1"
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
      }
    },
    "inserted": {
      "label": "Old binary pause",
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
    "pause-two-copy-0": {
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
    "pause-two-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "pause-two-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "pause-two-copy-2": {
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
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "pause-two",
      "origin": "hold"
    },
    "pause-two-copy-2": {
      "allocation": "pause-two",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "pause-two",
          "ordinal": 0
        },
        "layout": {
          "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "c289aced-ac0a-4d77-8bc0-121f54591304": {
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
              "allocation": "pause-two",
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
        "resume": null
      },
      "pause-two-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "pause-two",
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
                      "allocation": "pause-two",
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
        }
      }
    }
  }
}
');
INSERT INTO "revisions" VALUES('867471d7-8105-4dd6-9803-1f2518308283','fc69a81a-12c7-4899-bebe-306ca7cebd4f','undo','{
  "schema_version": 17,
  "project_id": "fae28ee2-efb0-4435-a95d-e99a4f7081cb",
  "revision_id": "867471d7-8105-4dd6-9803-1f2518308283",
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
  "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
  "nodes": {
    "c289aced-ac0a-4d77-8bc0-121f54591304": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "pause-two-copy-0",
          "inserted",
          "pause-two-copy-1"
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
      }
    },
    "inserted": {
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
    "pause-two-copy-0": {
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
    "pause-two-copy-1": {
      "label": "Pause",
      "kind": {
        "type": "retime",
        "child": "pause-two-copy-2",
        "duration": 5,
        "mapping": {
          "start": 3,
          "end": 8
        },
        "pitch": "preserve",
        "purpose": "partition"
      }
    },
    "pause-two-copy-2": {
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
      }
    }
  },
  "assets": {},
  "marks": {},
  "overrides": {},
  "audio_lineage": {
    "hold": {
      "allocation": "pause-two",
      "origin": "hold"
    },
    "pause-two-copy-2": {
      "allocation": "pause-two",
      "origin": "hold"
    }
  },
  "audio_bindings": {
    "timings": [
      {
        "id": {
          "allocation": "pause-two",
          "ordinal": 0
        },
        "layout": {
          "root": "c289aced-ac0a-4d77-8bc0-121f54591304",
          "rate": {
            "numerator": 30000,
            "denominator": 1001
          },
          "nodes": {
            "c289aced-ac0a-4d77-8bc0-121f54591304": {
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
              "allocation": "pause-two",
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
        "resume": null
      },
      "pause-two-copy-2": {
        "lattice": {
          "reference": {
            "timing": {
              "allocation": "pause-two",
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
                      "allocation": "pause-two",
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
        }
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
INSERT INTO "state" VALUES(1,'867471d7-8105-4dd6-9803-1f2518308283',2,'generic');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
