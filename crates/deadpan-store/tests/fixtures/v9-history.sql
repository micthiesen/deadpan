-- Genuine schema-9 acceptance qualification project, validated by the c224c2c binary.
-- Retains real six-object admission, acceptance, undo/redo and fallback reversion.
-- Existing schema-9 state exported through a consistent SQLite backup; no media bytes embedded.
PRAGMA application_id=1146113585;
PRAGMA user_version=9;
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
INSERT INTO "generation_attempt_heads" VALUES('probe-ad82362664564e1a97c381111aaad5aa',1,'attempt-1','attempt-1');
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
INSERT INTO "generation_attempts" VALUES('probe-ad82362664564e1a97c381111aaad5aa','attempt-1',1,'acceptance-probe-token','ready','inference',5,NULL,'{"native":{"reference":"outputs/native.mp4","sha256":"c9d34268df14d105bb4f3799e9bfc9b946ad153de43e6ee4a73a44bb7833be33","byte_length":4911319},"provenance":{"reference":"outputs/provenance.json","sha256":"7a74f15d64d30f98035b43193a4c624cb0102325b54e73eccf7405c5e7d5e18e","byte_length":37241},"video":{"frames":25,"frame_rate":{"numerator":24,"denominator":1},"width":768,"height":320},"provider":{"pack_id":"ltx-2.3-q4-development","pack_version":"56a5866d","runtime_id":"ltx-mlx-development","runtime_version":"0.15.8+deadpan1","seed":1}}',NULL,NULL,NULL);
CREATE TABLE generation_bundle_receipts (
    request_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    bundle TEXT NOT NULL CHECK (json_valid(bundle)),
    availability TEXT NOT NULL CHECK (availability IN ('present','evicted')),
    PRIMARY KEY (request_id,attempt_id),
    FOREIGN KEY (request_id,attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;
INSERT INTO "generation_bundle_receipts" VALUES('probe-ad82362664564e1a97c381111aaad5aa','attempt-1','{"native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"provenance_object":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"native_video":{"frames":25,"frame_rate":{"numerator":24,"denominator":1},"width":768,"height":320},"sampled_video":{"frames":30,"frame_rate":{"numerator":30000,"denominator":1001},"width":768,"height":320},"plan":{"schema_version":1,"operation":"bridge","interpolation":"linear","project":{"interior_frames":30,"frame_rate":{"numerator":30000,"denominator":1001}},"native":{"frame_count":25,"frame_rate":{"numerator":24,"denominator":1},"width":768,"height":320},"timing":{"requested_boundary_duration":{"numerator":"31031","denominator":"30000"},"actual_boundary_duration":{"numerator":"1","denominator":"1"},"retime_deviation":{"numerator":"-1031","denominator":"30000"}},"sampling":{"endpoint_policy":"interior_only"}},"provider":{"pack_id":"ltx-2.3-q4-development","pack_version":"56a5866d","runtime_id":"ltx-mlx-development","runtime_version":"0.15.8+deadpan1","seed":1},"native_sha256":"c9d34268df14d105bb4f3799e9bfc9b946ad153de43e6ee4a73a44bb7833be33","native_byte_length":4911319,"provenance_sha256":"7a74f15d64d30f98035b43193a4c624cb0102325b54e73eccf7405c5e7d5e18e","provenance_byte_length":37241,"validator":{"id":"native-ffv1","version":"bridge-3"},"availability":"present","admission":{"native_span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"sampled_span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"inputs":{"context_sha256":"54e84510a24b18437e79145c9cca90230bf0e38d48d72107006ac3f2e0de5eb4","manifest":{"content":{"algorithm":"blake3","digest":"fa905adba9f3b7b84cb1ade4b537ec3a2c7514c94bdcc3c4c16735f8d112fa92"},"byte_length":1350},"left":{"content":{"algorithm":"blake3","digest":"7074ad08d6a13fddcc615c7bf2fc4d7d3cd67320ae813e0f4af83e3c544ccf3e"},"byte_length":310963},"right":{"content":{"algorithm":"blake3","digest":"1546d7b105d6fcf262b93fabc5683642358930f897e0bd1c28617c1141482c3a"},"byte_length":310279}}}}','present');
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
INSERT INTO "generation_requests" VALUES('probe-ad82362664564e1a97c381111aaad5aa','probe-ad82362664564e1a97c381111aaad5aa','hold-1',1,'revision-1','54e84510a24b18437e79145c9cca90230bf0e38d48d72107006ac3f2e0de5eb4','{"video":{"frames":30,"frame_rate":{"numerator":30000,"denominator":1001},"width":768,"height":320},"conditioning":"bridge","motion":"still"}','{"pack_id":"ltx-2.3-q4-development","pack_version":"56a5866d","runtime_id":"ltx-mlx-development","runtime_version":"0.15.8+deadpan1","seed":1}','{"schema_version":1,"operation":"bridge","interpolation":"linear","project":{"interior_frames":30,"frame_rate":{"numerator":30000,"denominator":1001}},"native":{"frame_count":25,"frame_rate":{"numerator":24,"denominator":1},"width":768,"height":320},"timing":{"requested_boundary_duration":{"numerator":"31031","denominator":"30000"},"actual_boundary_duration":{"numerator":"1","denominator":"1"},"retime_deviation":{"numerator":"-1031","denominator":"30000"}},"sampling":{"endpoint_policy":"interior_only"}}','current');
CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
INSERT INTO "history" VALUES(1,NULL,'accepted','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"revision-1","new_revision":"accepted","command":{"command":"accept_generated_hold","node":"hold-1","artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"assets":{"accepted-native":{"label":"Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","content_hash":"blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":25},"accepted-sampled":{"label":"Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","content_hash":"blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":30}}}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"revision-1","to_revision":"accepted","nodes":{"hold-1":{"before":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{"accepted-native":{"before":null,"after":{"label":"Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","content_hash":"blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":25}},"accepted-sampled":{"before":null,"after":{"label":"Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","content_hash":"blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":30}}},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"accepted","to_revision":"revision-1","nodes":{"hold-1":{"before":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{"accepted-native":{"before":{"label":"Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","content_hash":"blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1041,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":25},"after":null},"accepted-sampled":{"before":{"label":"Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","content_hash":"blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b","video":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":1001,"time_base":{"numerator":1,"denominator":1000}}},"audio":null,"still_image":false,"frame_count":30},"after":null}},"marks":{},"overrides":{}},"changed_ids":["hold-1"],"duration_delta":0,"description":"Accept generated hold"}');
INSERT INTO "history" VALUES(2,1,'revert-to-fallback','{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","expected_revision":"redo-accept","new_revision":"revert-to-fallback","command":{"command":"revert_generated_hold","node":"hold-1"}}','{"forward":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"redo-accept","to_revision":"revert-to-fallback","nodes":{"hold-1":{"before":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}},"after":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"probe-ad82362664564e1a97c381111aaad5aa","from_revision":"revert-to-fallback","to_revision":"redo-accept","nodes":{"hold-1":{"before":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"background"},"audio":{"type":"silence"}}}},"after":{"label":"Development generation probe","kind":{"type":"hold","recipe":{"duration":30,"video":{"type":"generated","accepted":{"artifact":{"sampled_asset":"accepted-sampled","sampled_object":{"content":{"algorithm":"blake3","digest":"10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"},"byte_length":5450240},"native_asset":"accepted-native","native_object":{"content":{"algorithm":"blake3","digest":"21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"},"byte_length":4544157},"provenance":{"content":{"algorithm":"blake3","digest":"f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"},"byte_length":45014},"sampling":{"schema_version":1,"project_rate":{"numerator":30000,"denominator":1001},"native_rate":{"numerator":24,"denominator":1},"native_frame_count":25,"output_frame_count":30,"interpolation":"encoded_srgb_rgb8_linear_half_up"}},"fallback":{"type":"background"}}},"audio":{"type":"silence"}}}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["hold-1"],"duration_delta":0,"description":"Revert generated hold"}');
CREATE TABLE hold_request_clocks (
    hold_id TEXT PRIMARY KEY,
    high_water INTEGER NOT NULL
        CHECK (high_water BETWEEN 1 AND 9223372036854775807)
) STRICT;
INSERT INTO "hold_request_clocks" VALUES('hold-1',1);
CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('revision-1',NULL,'initial','{
  "schema_version": 5,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "revision-1",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
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
INSERT INTO "revisions" VALUES('accepted','revision-1','edit','{
  "schema_version": 5,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "accepted",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "generated",
            "accepted": {
              "artifact": {
                "sampled_asset": "accepted-sampled",
                "sampled_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"
                  },
                  "byte_length": 5450240
                },
                "native_asset": "accepted-native",
                "native_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"
                  },
                  "byte_length": 4544157
                },
                "provenance": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"
                  },
                  "byte_length": 45014
                },
                "sampling": {
                  "schema_version": 1,
                  "project_rate": {
                    "numerator": 30000,
                    "denominator": 1001
                  },
                  "native_rate": {
                    "numerator": 24,
                    "denominator": 1
                  },
                  "native_frame_count": 25,
                  "output_frame_count": 30,
                  "interpolation": "encoded_srgb_rgb8_linear_half_up"
                }
              },
              "fallback": {
                "type": "background"
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('undo-accept','accepted','undo','{
  "schema_version": 5,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "undo-accept",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
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
INSERT INTO "revisions" VALUES('redo-accept','undo-accept','redo','{
  "schema_version": 5,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "redo-accept",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
          "video": {
            "type": "generated",
            "accepted": {
              "artifact": {
                "sampled_asset": "accepted-sampled",
                "sampled_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b"
                  },
                  "byte_length": 5450240
                },
                "native_asset": "accepted-native",
                "native_object": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a"
                  },
                  "byte_length": 4544157
                },
                "provenance": {
                  "content": {
                    "algorithm": "blake3",
                    "digest": "f2d2f150831fb0ac67eab44ba5e40ace721c79b6c16a7f752be121b679a70724"
                  },
                  "byte_length": 45014
                },
                "sampling": {
                  "schema_version": 1,
                  "project_rate": {
                    "numerator": 30000,
                    "denominator": 1001
                  },
                  "native_rate": {
                    "numerator": 24,
                    "denominator": 1
                  },
                  "native_frame_count": 25,
                  "output_frame_count": 30,
                  "interpolation": "encoded_srgb_rgb8_linear_half_up"
                }
              },
              "fallback": {
                "type": "background"
              }
            }
          },
          "audio": {
            "type": "silence"
          }
        }
      }
    }
  },
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('revert-to-fallback','redo-accept','edit','{
  "schema_version": 5,
  "project_id": "probe-ad82362664564e1a97c381111aaad5aa",
  "revision_id": "revert-to-fallback",
  "presentation_basis": {
    "width": 768,
    "height": 320,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "root": "acceptance-probe-root",
  "nodes": {
    "acceptance-probe-root": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "hold-1"
        ]
      }
    },
    "hold-1": {
      "label": "Development generation probe",
      "kind": {
        "type": "hold",
        "recipe": {
          "duration": 30,
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
  "assets": {
    "accepted-native": {
      "label": "Generated 21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "content_hash": "blake3:21cba6b5ade6e09ae3822cf1d31f103084d9880d884518a496e8c4d553f5c36a",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1041,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 25
    },
    "accepted-sampled": {
      "label": "Generated 10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "content_hash": "blake3:10b19fd1700b4163dbdc73c5c9d6d3710fd9fb087f8dfa9d235a3c25c0c04b3b",
      "video": {
        "start": {
          "ticks": 0,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        },
        "end": {
          "ticks": 1001,
          "time_base": {
            "numerator": 1,
            "denominator": 1000
          }
        }
      },
      "audio": null,
      "still_image": false,
      "frame_count": 30
    }
  },
  "marks": {},
  "overrides": {}
}
');
CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id)
        ) STRICT;
INSERT INTO "state" VALUES(1,'revert-to-fallback',2);
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
