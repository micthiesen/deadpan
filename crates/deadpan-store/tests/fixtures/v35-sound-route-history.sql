PRAGMA application_id=1146113585;
PRAGMA user_version=35;
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
INSERT INTO "history" VALUES(1,NULL,'core29-import','{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","expected_revision":"974a704c-762f-46fa-b908-0d3f0eddf2cd","new_revision":"core29-import","command":{"command":"import_source","id":"camera","asset":{"label":"Measured camera","content_hash":"blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f","video":{"start":{"ticks":60060,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":180180,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":288288,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"},"insertion":{"parent":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","index":0,"node":"clip","label":"Original","source":{"duration":121,"video":{"type":"stream","asset":"camera","span":{"start":{"ticks":60060,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":180180,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"640","denominator":"1001"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":288288,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120760","denominator":"1001"}},"link":"linked","audio_offset":0}},"primary":{"type":"keep_basis"}}}','{"forward":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"974a704c-762f-46fa-b908-0d3f0eddf2cd","to_revision":"core29-import","presentation":{"before":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null}},"after":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":{"asset":"camera","qualification":"38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"}}}},"nodes":{"9ad8a1de-32dd-4182-8df7-319f6dee8b91":{"before":{"label":"Sequence","kind":{"type":"sequence","children":[]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["clip"]}}},"clip":{"before":null,"after":{"label":"Original","kind":{"type":"source","source":{"duration":121,"video":{"type":"stream","asset":"camera","span":{"start":{"ticks":60060,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":180180,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"640","denominator":"1001"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":288288,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120760","denominator":"1001"}},"link":"linked","audio_offset":0}}}}},"assets":{"camera":{"before":null,"after":{"label":"Measured camera","content_hash":"blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f","video":{"start":{"ticks":60060,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":180180,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":288288,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"}}},"marks":{},"overrides":{}},"inverse":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-import","to_revision":"974a704c-762f-46fa-b908-0d3f0eddf2cd","presentation":{"before":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":{"asset":"camera","qualification":"38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"}}},"after":{"basis":{"width":320,"height":180,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},"state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null}}},"nodes":{"9ad8a1de-32dd-4182-8df7-319f6dee8b91":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["clip"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":[]}}},"clip":{"before":{"label":"Original","kind":{"type":"source","source":{"duration":121,"video":{"type":"stream","asset":"camera","span":{"start":{"ticks":60060,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":180180,"time_base":{"numerator":1,"denominator":30000}}}},"video_mapping":{"type":"placement","start":{"numerator":"640","denominator":"1001"},"frames":{"numerator":"120","denominator":"1"},"endpoints":"hold_adjacent"},"audio":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":288288,"time_base":{"numerator":1,"denominator":48000}}}},"audio_mapping":{"type":"placement","start":{"numerator":"0","denominator":"1"},"frames":{"numerator":"120760","denominator":"1001"}},"link":"linked","audio_offset":0}}},"after":null}},"assets":{"camera":{"before":{"label":"Measured camera","content_hash":"blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f","video":{"start":{"ticks":60060,"time_base":{"numerator":1,"denominator":30000}},"end":{"ticks":180180,"time_base":{"numerator":1,"denominator":30000}}},"audio":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":288288,"time_base":{"numerator":1,"denominator":48000}}},"still_image":false,"frame_count":120,"source_qualification":"38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"},"after":null}},"marks":{},"overrides":{}},"changed_ids":["9ad8a1de-32dd-4182-8df7-319f6dee8b91","clip"],"duration_delta":121,"description":"Import source media"}');
INSERT INTO "history" VALUES(2,1,'core29-repeat','{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","expected_revision":"core29-import","new_revision":"core29-repeat","command":{"command":"wrap_repeat","node":"clip","id":"repeat","plays":2,"gap":null,"anchor_policy":"first"}}','{"forward":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-import","to_revision":"core29-repeat","nodes":{"9ad8a1de-32dd-4182-8df7-319f6dee8b91":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["clip"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}}},"repeat":{"before":null,"after":{"label":"Repeat","kind":{"type":"repeat","child":"clip","iterations":{"runs":[{"allocation":"core29-repeat","first":0,"count":2}]},"gap":null}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-repeat","to_revision":"core29-import","nodes":{"9ad8a1de-32dd-4182-8df7-319f6dee8b91":{"before":{"label":"Sequence","kind":{"type":"sequence","children":["repeat"]}},"after":{"label":"Sequence","kind":{"type":"sequence","children":["clip"]}}},"repeat":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"clip","iterations":{"runs":[{"allocation":"core29-repeat","first":0,"count":2}]},"gap":null}},"after":null}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["9ad8a1de-32dd-4182-8df7-319f6dee8b91","repeat"],"duration_delta":121,"description":"Wrap repeat"}');
INSERT INTO "history" VALUES(3,2,'core29-add-impact','{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","expected_revision":"core29-repeat","new_revision":"core29-add-impact","command":{"command":"set_sound","id":"impact","event":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":137,"gain_millidecibels":-3000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"}}}','{"forward":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-repeat","to_revision":"core29-add-impact","nodes":{},"assets":{},"marks":{},"sounds":{"impact":{"before":null,"after":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":137,"gain_millidecibels":-3000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"}}},"overrides":{}},"inverse":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-add-impact","to_revision":"core29-repeat","nodes":{},"assets":{},"marks":{},"sounds":{"impact":{"before":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":137,"gain_millidecibels":-3000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"},"after":null}},"overrides":{}},"changed_ids":["9ad8a1de-32dd-4182-8df7-319f6dee8b91"],"duration_delta":0,"description":"Set sound event"}');
INSERT INTO "history" VALUES(4,3,'core29-abandoned-impact','{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","expected_revision":"core29-add-impact","new_revision":"core29-abandoned-impact","command":{"command":"set_sound","id":"impact","event":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Abandoned impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":941,"gain_millidecibels":-6000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"}}}','{"forward":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-add-impact","to_revision":"core29-abandoned-impact","nodes":{},"assets":{},"marks":{},"sounds":{"impact":{"before":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":137,"gain_millidecibels":-3000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"},"after":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Abandoned impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":941,"gain_millidecibels":-6000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"}}},"overrides":{}},"inverse":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-abandoned-impact","to_revision":"core29-add-impact","nodes":{},"assets":{},"marks":{},"sounds":{"impact":{"before":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Abandoned impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":941,"gain_millidecibels":-6000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"},"after":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":137,"gain_millidecibels":-3000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"}}},"overrides":{}},"changed_ids":["9ad8a1de-32dd-4182-8df7-319f6dee8b91"],"duration_delta":0,"description":"Set sound event"}');
INSERT INTO "history" VALUES(5,3,'core29-update-impact','{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","expected_revision":"cf61eefb-50c4-4f68-8b62-355965189a0e","new_revision":"core29-update-impact","command":{"command":"set_sound","id":"impact","event":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Retained impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":277,"gain_millidecibels":-9000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"}}}','{"forward":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"cf61eefb-50c4-4f68-8b62-355965189a0e","to_revision":"core29-update-impact","nodes":{},"assets":{},"marks":{},"sounds":{"impact":{"before":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":137,"gain_millidecibels":-3000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"},"after":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Retained impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":277,"gain_millidecibels":-9000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"}}},"overrides":{}},"inverse":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-update-impact","to_revision":"cf61eefb-50c4-4f68-8b62-355965189a0e","nodes":{},"assets":{},"marks":{},"sounds":{"impact":{"before":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Retained impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":277,"gain_millidecibels":-9000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"},"after":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":137,"gain_millidecibels":-3000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"}}},"overrides":{}},"changed_ids":["9ad8a1de-32dd-4182-8df7-319f6dee8b91"],"duration_delta":0,"description":"Set sound event"}');
INSERT INTO "history" VALUES(6,5,'core29-add-bed','{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","expected_revision":"core29-update-impact","new_revision":"core29-add-bed","command":{"command":"set_sound","id":"bed","event":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Bed","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":13,"gain_millidecibels":-12000,"start_edge":"automatic","end_edge":"automatic","overflow":"reject"}}}','{"forward":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-update-impact","to_revision":"core29-add-bed","nodes":{},"assets":{},"marks":{},"sounds":{"bed":{"before":null,"after":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Bed","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":13,"gain_millidecibels":-12000,"start_edge":"automatic","end_edge":"automatic","overflow":"reject"}}},"overrides":{}},"inverse":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-add-bed","to_revision":"core29-update-impact","nodes":{},"assets":{},"marks":{},"sounds":{"bed":{"before":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Bed","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":13,"gain_millidecibels":-12000,"start_edge":"automatic","end_edge":"automatic","overflow":"reject"},"after":null}},"overrides":{}},"changed_ids":["9ad8a1de-32dd-4182-8df7-319f6dee8b91"],"duration_delta":0,"description":"Set sound event"}');
INSERT INTO "history" VALUES(7,6,'core29-label','{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","expected_revision":"core29-add-bed","new_revision":"core29-label","command":{"command":"rename","node":"repeat","label":"Two plays with overlays"}}','{"forward":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-add-bed","to_revision":"core29-label","nodes":{"repeat":{"before":{"label":"Repeat","kind":{"type":"repeat","child":"clip","iterations":{"runs":[{"allocation":"core29-repeat","first":0,"count":2}]},"gap":null}},"after":{"label":"Two plays with overlays","kind":{"type":"repeat","child":"clip","iterations":{"runs":[{"allocation":"core29-repeat","first":0,"count":2}]},"gap":null}}}},"assets":{},"marks":{},"overrides":{}},"inverse":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-label","to_revision":"core29-add-bed","nodes":{"repeat":{"before":{"label":"Two plays with overlays","kind":{"type":"repeat","child":"clip","iterations":{"runs":[{"allocation":"core29-repeat","first":0,"count":2}]},"gap":null}},"after":{"label":"Repeat","kind":{"type":"repeat","child":"clip","iterations":{"runs":[{"allocation":"core29-repeat","first":0,"count":2}]},"gap":null}}}},"assets":{},"marks":{},"overrides":{}},"changed_ids":["repeat"],"duration_delta":0,"description":"Rename beat"}');
INSERT INTO "history" VALUES(8,7,'core29-delete-impact','{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","expected_revision":"core29-label","new_revision":"core29-delete-impact","command":{"command":"delete_sound","id":"impact"}}','{"forward":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-label","to_revision":"core29-delete-impact","nodes":{},"assets":{},"marks":{},"sounds":{"impact":{"before":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Retained impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":277,"gain_millidecibels":-9000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"},"after":null}},"overrides":{}},"inverse":{"project_id":"e57fe5e8-ab20-4de1-828a-b7fd56372b7d","from_revision":"core29-delete-impact","to_revision":"core29-label","nodes":{},"assets":{},"marks":{},"sounds":{"impact":{"before":null,"after":{"owner":"9ad8a1de-32dd-4182-8df7-319f6dee8b91","label":"Retained impact","source":{"asset":"camera","span":{"start":{"ticks":95072,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":191680,"time_base":{"numerator":1,"denominator":48000}}}},"mapping":{"type":"duration","frames":{"numerator":"60380","denominator":"1001"}},"offset":277,"gain_millidecibels":-9000,"start_edge":"automatic","end_edge":"hard","overflow":"reject"}}},"overrides":{}},"changed_ids":["9ad8a1de-32dd-4182-8df7-319f6dee8b91"],"duration_delta":0,"description":"Delete sound event"}');
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
INSERT INTO "original_media" VALUES('blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f',1,'{"object":{"content":{"algorithm":"blake3","digest":"2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f"},"byte_length":39181},"sha256":[21,4,232,196,28,46,142,51,207,88,139,190,70,231,199,159,159,181,7,67,117,51,245,214,205,111,40,11,107,253,177,32],"label":"offset-bframes.mp4","version":1,"managed":true,"linked":null}');
CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
INSERT INTO "redo" VALUES(1,8);
CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
INSERT INTO "revisions" VALUES('974a704c-762f-46fa-b908-0d3f0eddf2cd',NULL,'initial','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "974a704c-762f-46fa-b908-0d3f0eddf2cd",
  "presentation_basis": {
    "width": 320,
    "height": 180,
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
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
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
INSERT INTO "revisions" VALUES('core29-import','974a704c-762f-46fa-b908-0d3f0eddf2cd','edit','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "core29-import",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "clip"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('core29-repeat','core29-import','edit','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "core29-repeat",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('core29-add-impact','core29-repeat','edit','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "core29-add-impact",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "impact": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Impact",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 137,
      "gain_millidecibels": -3000,
      "start_edge": "automatic",
      "end_edge": "hard",
      "overflow": "reject"
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('core29-abandoned-impact','core29-add-impact','edit','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "core29-abandoned-impact",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "impact": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Abandoned impact",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 941,
      "gain_millidecibels": -6000,
      "start_edge": "automatic",
      "end_edge": "hard",
      "overflow": "reject"
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('cf61eefb-50c4-4f68-8b62-355965189a0e','core29-abandoned-impact','undo','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "cf61eefb-50c4-4f68-8b62-355965189a0e",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "impact": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Impact",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 137,
      "gain_millidecibels": -3000,
      "start_edge": "automatic",
      "end_edge": "hard",
      "overflow": "reject"
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('core29-update-impact','cf61eefb-50c4-4f68-8b62-355965189a0e','edit','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "core29-update-impact",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "impact": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Retained impact",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 277,
      "gain_millidecibels": -9000,
      "start_edge": "automatic",
      "end_edge": "hard",
      "overflow": "reject"
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('core29-add-bed','core29-update-impact','edit','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "core29-add-bed",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Repeat",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "bed": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Bed",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 13,
      "gain_millidecibels": -12000,
      "start_edge": "automatic",
      "end_edge": "automatic",
      "overflow": "reject"
    },
    "impact": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Retained impact",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 277,
      "gain_millidecibels": -9000,
      "start_edge": "automatic",
      "end_edge": "hard",
      "overflow": "reject"
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('core29-label','core29-add-bed','edit','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "core29-label",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Two plays with overlays",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "bed": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Bed",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 13,
      "gain_millidecibels": -12000,
      "start_edge": "automatic",
      "end_edge": "automatic",
      "overflow": "reject"
    },
    "impact": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Retained impact",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 277,
      "gain_millidecibels": -9000,
      "start_edge": "automatic",
      "end_edge": "hard",
      "overflow": "reject"
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('core29-delete-impact','core29-label','edit','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "core29-delete-impact",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Two plays with overlays",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "bed": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Bed",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 13,
      "gain_millidecibels": -12000,
      "start_edge": "automatic",
      "end_edge": "automatic",
      "overflow": "reject"
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('32cd8a2f-c377-4ba0-83c8-b957f8439dbb','core29-delete-impact','undo','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "32cd8a2f-c377-4ba0-83c8-b957f8439dbb",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Two plays with overlays",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "bed": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Bed",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 13,
      "gain_millidecibels": -12000,
      "start_edge": "automatic",
      "end_edge": "automatic",
      "overflow": "reject"
    },
    "impact": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Retained impact",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 277,
      "gain_millidecibels": -9000,
      "start_edge": "automatic",
      "end_edge": "hard",
      "overflow": "reject"
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('c1291142-eee2-416f-8595-70edab5f772c','32cd8a2f-c377-4ba0-83c8-b957f8439dbb','redo','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "c1291142-eee2-416f-8595-70edab5f772c",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Two plays with overlays",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "bed": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Bed",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 13,
      "gain_millidecibels": -12000,
      "start_edge": "automatic",
      "end_edge": "automatic",
      "overflow": "reject"
    }
  },
  "overrides": {}
}
');
INSERT INTO "revisions" VALUES('04c85d5b-8a06-412b-b572-9a331d9ba34d','c1291142-eee2-416f-8595-70edab5f772c','undo','{
  "schema_version": 29,
  "project_id": "e57fe5e8-ab20-4de1-828a-b7fd56372b7d",
  "revision_id": "04c85d5b-8a06-412b-b572-9a331d9ba34d",
  "presentation_basis": {
    "width": 320,
    "height": 180,
    "frame_rate": {
      "numerator": 30000,
      "denominator": 1001
    },
    "color_policy": "sdr_rec709"
  },
  "basis_state": {
    "rate_origin": "explicit",
    "geometry_origin": "explicit",
    "primary": {
      "asset": "camera",
      "qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "root": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
  "nodes": {
    "9ad8a1de-32dd-4182-8df7-319f6dee8b91": {
      "label": "Sequence",
      "kind": {
        "type": "sequence",
        "children": [
          "repeat"
        ]
      }
    },
    "clip": {
      "label": "Original",
      "kind": {
        "type": "source",
        "source": {
          "duration": 121,
          "video": {
            "type": "stream",
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 60060,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              },
              "end": {
                "ticks": 180180,
                "time_base": {
                  "numerator": 1,
                  "denominator": 30000
                }
              }
            }
          },
          "video_mapping": {
            "type": "placement",
            "start": {
              "numerator": "640",
              "denominator": "1001"
            },
            "frames": {
              "numerator": "120",
              "denominator": "1"
            },
            "endpoints": "hold_adjacent"
          },
          "audio": {
            "asset": "camera",
            "span": {
              "start": {
                "ticks": 95072,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              },
              "end": {
                "ticks": 288288,
                "time_base": {
                  "numerator": 1,
                  "denominator": 48000
                }
              }
            }
          },
          "audio_mapping": {
            "type": "placement",
            "start": {
              "numerator": "0",
              "denominator": "1"
            },
            "frames": {
              "numerator": "120760",
              "denominator": "1001"
            }
          },
          "link": "linked",
          "audio_offset": 0
        }
      }
    },
    "repeat": {
      "label": "Two plays with overlays",
      "kind": {
        "type": "repeat",
        "child": "clip",
        "iterations": {
          "runs": [
            {
              "allocation": "core29-repeat",
              "first": 0,
              "count": 2
            }
          ]
        },
        "gap": null
      }
    }
  },
  "assets": {
    "camera": {
      "label": "Measured camera",
      "content_hash": "blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f",
      "video": {
        "start": {
          "ticks": 60060,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        },
        "end": {
          "ticks": 180180,
          "time_base": {
            "numerator": 1,
            "denominator": 30000
          }
        }
      },
      "audio": {
        "start": {
          "ticks": 95072,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        },
        "end": {
          "ticks": 288288,
          "time_base": {
            "numerator": 1,
            "denominator": 48000
          }
        }
      },
      "still_image": false,
      "frame_count": 120,
      "source_qualification": "38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4"
    }
  },
  "marks": {},
  "sounds": {
    "bed": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Bed",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 13,
      "gain_millidecibels": -12000,
      "start_edge": "automatic",
      "end_edge": "automatic",
      "overflow": "reject"
    },
    "impact": {
      "owner": "9ad8a1de-32dd-4182-8df7-319f6dee8b91",
      "label": "Retained impact",
      "source": {
        "asset": "camera",
        "span": {
          "start": {
            "ticks": 95072,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          },
          "end": {
            "ticks": 191680,
            "time_base": {
              "numerator": 1,
              "denominator": 48000
            }
          }
        }
      },
      "mapping": {
        "type": "duration",
        "frames": {
          "numerator": "60380",
          "denominator": "1001"
        }
      },
      "offset": 277,
      "gain_millidecibels": -9000,
      "start_edge": "automatic",
      "end_edge": "hard",
      "overflow": "reject"
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
INSERT INTO "source_qualifications" VALUES('38776b6eae9a3ce89f1126abbcd6e3cd74a68bb05d09489ee58a548f3499dfe4','blake3:2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f','{"content":{"algorithm":"blake3","digest":"2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f"},"byte_length":39181}',X'7B22736368656D615F76657273696F6E223A312C226465636F6465725F636F6E7472616374223A2266666D7065672D382E302E332F736F757263652D6465636F6465642D7631222C2274696D696E675F706F6C6963795F76657273696F6E223A312C22636F6E74656E74223A7B22736861323536223A5B32312C342C3233322C3139362C32382C34362C3134322C35312C3230372C38382C3133392C3139302C37302C3233312C3139392C3135392C3135392C3138312C372C36372C3131372C35312C3234352C3231342C3230352C3131312C34302C31312C3130372C3235332C3137372C33325D2C22627974655F6C656E677468223A33393138317D2C226F726967696E5F7365636F6E6473223A7B226E756D657261746F72223A2232393731222C2264656E6F6D696E61746F72223A2231353030227D2C22766964656F223A7B22696E646578223A7B22736368656D615F76657273696F6E223A312C22636F6E74656E74223A7B22736861323536223A5B32312C342C3233322C3139362C32382C34362C3134322C35312C3230372C38382C3133392C3139302C37302C3233312C3139392C3135392C3135392C3138312C372C36372C3131372C35312C3234352C3231342C3230352C3131312C34302C31312C3130372C3235332C3137372C33325D2C22627974655F6C656E677468223A33393138317D2C2273747265616D5F696E646578223A302C22696E646578223A7B226173736574223A227175616C69666965642D736F75726365222C2274696D655F62617365223A7B226E756D657261746F72223A312C2264656E6F6D696E61746F72223A33303030307D2C226672616D6573223A5B7B226964656E74697479223A302C22707473223A36303036302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36303036307D2C7B226964656E74697479223A312C22707473223A36313036312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36313036317D2C7B226964656E74697479223A322C22707473223A36323036322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36323036327D2C7B226964656E74697479223A332C22707473223A36333036332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36333036337D2C7B226964656E74697479223A342C22707473223A36343036342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36343036347D2C7B226964656E74697479223A352C22707473223A36353036352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36353036357D2C7B226964656E74697479223A362C22707473223A36363036362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36363036367D2C7B226964656E74697479223A372C22707473223A36373036372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36373036377D2C7B226964656E74697479223A382C22707473223A36383036382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36383036387D2C7B226964656E74697479223A392C22707473223A36393036392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A36393036397D2C7B226964656E74697479223A31302C22707473223A37303037302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A37303037307D2C7B226964656E74697479223A31312C22707473223A37313037312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A37313037317D2C7B226964656E74697479223A31322C22707473223A37323037322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A37323037327D2C7B226964656E74697479223A31332C22707473223A37333037332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A37333037337D2C7B226964656E74697479223A31342C22707473223A37343037342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A302C226465636F64655F74696D657374616D70223A37343037347D2C7B226964656E74697479223A31352C22707473223A37353037352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A37353037357D2C7B226964656E74697479223A31362C22707473223A37363037362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A37363037367D2C7B226964656E74697479223A31372C22707473223A37373037372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A37373037377D2C7B226964656E74697479223A31382C22707473223A37383037382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A37383037387D2C7B226964656E74697479223A31392C22707473223A37393037392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A37393037397D2C7B226964656E74697479223A32302C22707473223A38303038302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38303038307D2C7B226964656E74697479223A32312C22707473223A38313038312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38313038317D2C7B226964656E74697479223A32322C22707473223A38323038322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38323038327D2C7B226964656E74697479223A32332C22707473223A38333038332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38333038337D2C7B226964656E74697479223A32342C22707473223A38343038342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38343038347D2C7B226964656E74697479223A32352C22707473223A38353038352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38353038357D2C7B226964656E74697479223A32362C22707473223A38363038362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38363038367D2C7B226964656E74697479223A32372C22707473223A38373038372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38373038377D2C7B226964656E74697479223A32382C22707473223A38383038382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38383038387D2C7B226964656E74697479223A32392C22707473223A38393038392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A31352C226465636F64655F74696D657374616D70223A38393038397D2C7B226964656E74697479223A33302C22707473223A39303039302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39303039307D2C7B226964656E74697479223A33312C22707473223A39313039312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39313039317D2C7B226964656E74697479223A33322C22707473223A39323039322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39323039327D2C7B226964656E74697479223A33332C22707473223A39333039332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39333039337D2C7B226964656E74697479223A33342C22707473223A39343039342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39343039347D2C7B226964656E74697479223A33352C22707473223A39353039352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39353039357D2C7B226964656E74697479223A33362C22707473223A39363039362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39363039367D2C7B226964656E74697479223A33372C22707473223A39373039372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39373039377D2C7B226964656E74697479223A33382C22707473223A39383039382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39383039387D2C7B226964656E74697479223A33392C22707473223A39393039392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A39393039397D2C7B226964656E74697479223A34302C22707473223A3130303130302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A3130303130307D2C7B226964656E74697479223A34312C22707473223A3130313130312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A3130313130317D2C7B226964656E74697479223A34322C22707473223A3130323130322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A3130323130327D2C7B226964656E74697479223A34332C22707473223A3130333130332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A3130333130337D2C7B226964656E74697479223A34342C22707473223A3130343130342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A33302C226465636F64655F74696D657374616D70223A3130343130347D2C7B226964656E74697479223A34352C22707473223A3130353130352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130353130357D2C7B226964656E74697479223A34362C22707473223A3130363130362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130363130367D2C7B226964656E74697479223A34372C22707473223A3130373130372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130373130377D2C7B226964656E74697479223A34382C22707473223A3130383130382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130383130387D2C7B226964656E74697479223A34392C22707473223A3130393130392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3130393130397D2C7B226964656E74697479223A35302C22707473223A3131303131302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131303131307D2C7B226964656E74697479223A35312C22707473223A3131313131312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131313131317D2C7B226964656E74697479223A35322C22707473223A3131323131322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131323131327D2C7B226964656E74697479223A35332C22707473223A3131333131332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131333131337D2C7B226964656E74697479223A35342C22707473223A3131343131342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131343131347D2C7B226964656E74697479223A35352C22707473223A3131353131352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131353131357D2C7B226964656E74697479223A35362C22707473223A3131363131362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131363131367D2C7B226964656E74697479223A35372C22707473223A3131373131372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131373131377D2C7B226964656E74697479223A35382C22707473223A3131383131382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131383131387D2C7B226964656E74697479223A35392C22707473223A3131393131392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A34352C226465636F64655F74696D657374616D70223A3131393131397D2C7B226964656E74697479223A36302C22707473223A3132303132302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132303132307D2C7B226964656E74697479223A36312C22707473223A3132313132312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132313132317D2C7B226964656E74697479223A36322C22707473223A3132323132322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132323132327D2C7B226964656E74697479223A36332C22707473223A3132333132332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132333132337D2C7B226964656E74697479223A36342C22707473223A3132343132342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132343132347D2C7B226964656E74697479223A36352C22707473223A3132353132352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132353132357D2C7B226964656E74697479223A36362C22707473223A3132363132362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132363132367D2C7B226964656E74697479223A36372C22707473223A3132373132372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132373132377D2C7B226964656E74697479223A36382C22707473223A3132383132382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132383132387D2C7B226964656E74697479223A36392C22707473223A3132393132392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3132393132397D2C7B226964656E74697479223A37302C22707473223A3133303133302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133303133307D2C7B226964656E74697479223A37312C22707473223A3133313133312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133313133317D2C7B226964656E74697479223A37322C22707473223A3133323133322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133323133327D2C7B226964656E74697479223A37332C22707473223A3133333133332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133333133337D2C7B226964656E74697479223A37342C22707473223A3133343133342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A36302C226465636F64655F74696D657374616D70223A3133343133347D2C7B226964656E74697479223A37352C22707473223A3133353133352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3133353133357D2C7B226964656E74697479223A37362C22707473223A3133363133362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3133363133367D2C7B226964656E74697479223A37372C22707473223A3133373133372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3133373133377D2C7B226964656E74697479223A37382C22707473223A3133383133382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3133383133387D2C7B226964656E74697479223A37392C22707473223A3133393133392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3133393133397D2C7B226964656E74697479223A38302C22707473223A3134303134302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134303134307D2C7B226964656E74697479223A38312C22707473223A3134313134312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134313134317D2C7B226964656E74697479223A38322C22707473223A3134323134322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134323134327D2C7B226964656E74697479223A38332C22707473223A3134333134332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134333134337D2C7B226964656E74697479223A38342C22707473223A3134343134342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134343134347D2C7B226964656E74697479223A38352C22707473223A3134353134352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134353134357D2C7B226964656E74697479223A38362C22707473223A3134363134362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134363134367D2C7B226964656E74697479223A38372C22707473223A3134373134372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134373134377D2C7B226964656E74697479223A38382C22707473223A3134383134382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134383134387D2C7B226964656E74697479223A38392C22707473223A3134393134392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A37352C226465636F64655F74696D657374616D70223A3134393134397D2C7B226964656E74697479223A39302C22707473223A3135303135302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135303135307D2C7B226964656E74697479223A39312C22707473223A3135313135312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135313135317D2C7B226964656E74697479223A39322C22707473223A3135323135322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135323135327D2C7B226964656E74697479223A39332C22707473223A3135333135332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135333135337D2C7B226964656E74697479223A39342C22707473223A3135343135342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135343135347D2C7B226964656E74697479223A39352C22707473223A3135353135352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135353135357D2C7B226964656E74697479223A39362C22707473223A3135363135362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135363135367D2C7B226964656E74697479223A39372C22707473223A3135373135372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135373135377D2C7B226964656E74697479223A39382C22707473223A3135383135382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135383135387D2C7B226964656E74697479223A39392C22707473223A3135393135392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3135393135397D2C7B226964656E74697479223A3130302C22707473223A3136303136302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3136303136307D2C7B226964656E74697479223A3130312C22707473223A3136313136312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3136313136317D2C7B226964656E74697479223A3130322C22707473223A3136323136322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3136323136327D2C7B226964656E74697479223A3130332C22707473223A3136333136332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3136333136337D2C7B226964656E74697479223A3130342C22707473223A3136343136342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A39302C226465636F64655F74696D657374616D70223A3136343136347D2C7B226964656E74697479223A3130352C22707473223A3136353136352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A747275652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3136353136357D2C7B226964656E74697479223A3130362C22707473223A3136363136362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3136363136367D2C7B226964656E74697479223A3130372C22707473223A3136373136372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3136373136377D2C7B226964656E74697479223A3130382C22707473223A3136383136382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3136383136387D2C7B226964656E74697479223A3130392C22707473223A3136393136392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3136393136397D2C7B226964656E74697479223A3131302C22707473223A3137303137302C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3137303137307D2C7B226964656E74697479223A3131312C22707473223A3137313137312C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3137313137317D2C7B226964656E74697479223A3131322C22707473223A3137323137322C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3137323137327D2C7B226964656E74697479223A3131332C22707473223A3137333137332C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3137333137337D2C7B226964656E74697479223A3131342C22707473223A3137343137342C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3137343137347D2C7B226964656E74697479223A3131352C22707473223A3137353137352C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3137353137357D2C7B226964656E74697479223A3131362C22707473223A3137363137362C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3137363137367D2C7B226964656E74697479223A3131372C22707473223A3137373137372C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3137373137377D2C7B226964656E74697479223A3131382C22707473223A3137383137382C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A3137383137387D2C7B226964656E74697479223A3131392C22707473223A3137393137392C227265706F727465645F6475726174696F6E223A313030312C226B65796672616D65223A66616C73652C227365656B5F66726F6D223A3130352C226465636F64655F74696D657374616D70223A6E756C6C7D5D2C227465726D696E616C5F656E64223A3138303138302C227465726D696E616C5F70726F76656E616E6365223A226465636F6465645F6672616D655F6475726174696F6E227D7D2C22696E746572707265746174696F6E223A7B227769647468223A3332302C22686569676874223A3138302C2273747265616D5F696E646578223A302C2274696D655F626173655F6E756D223A312C2274696D655F626173655F64656E223A33303030302C2273616D706C655F6173706563745F6E756D223A312C2273616D706C655F6173706563745F64656E223A312C22726F746174696F6E5F717561727465725F7475726E73223A302C22636F6C6F72223A7B2272616E6765223A226C696D69746564222C226D6174726978223A226274373039222C227472616E73666572223A226274373039222C227072696D6172696573223A226274373039227D2C22636F646563223A2268323634222C22706978656C5F666F726D6174223A2279757634323070222C2273747265616D5F7374617274223A36303036302C2273747265616D5F6475726174696F6E223A3132303132302C22636F6E7461696E65725F7374617274223A6E756C6C2C22636F6E7461696E65725F6475726174696F6E223A6E756C6C2C22617564696F5F73747265616D73223A5B7B2273747265616D5F696E646578223A312C22636F646563223A22616163222C2274696D655F626173655F6E756D223A312C2274696D655F626173655F64656E223A34383030302C2273747265616D5F7374617274223A39353037322C2273747265616D5F6475726174696F6E223A3139333231362C2273616D706C655F72617465223A34383030302C226368616E6E656C5F636F756E74223A327D5D7D7D2C22617564696F223A7B22736368656D615F76657273696F6E223A312C226465636F6465725F636F6E7472616374223A2266666D7065672D382E302E332F617564696F2D6D616E75616C2D736B69702D7631222C22636F6E74656E74223A7B22736861323536223A5B32312C342C3233322C3139362C32382C34362C3134322C35312C3230372C38382C3133392C3139302C37302C3233312C3139392C3135392C3135392C3138312C372C36372C3131372C35312C3234352C3231342C3230352C3131312C34302C31312C3130372C3235332C3137372C33325D2C22627974655F6C656E677468223A33393138317D2C2273747265616D223A7B2273747265616D5F696E646578223A312C22636F646563223A22616163222C2274696D655F62617365223A7B226E756D657261746F72223A312C2264656E6F6D696E61746F72223A34383030307D2C2273616D706C655F72617465223A34383030302C226368616E6E656C5F6C61796F7574223A7B226F72646572223A226E6174697665222C226368616E6E656C73223A322C226D61736B223A337D2C2273747265616D5F7374617274223A39353037322C2273747265616D5F6475726174696F6E223A3139333231362C22696E697469616C5F70616464696E67223A302C22747261696C696E675F70616464696E67223A302C227365656B5F707265726F6C6C223A307D2C226F62736572766174696F6E73223A5B7B22707473223A39353037322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39353037322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39363039362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39363039362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39373132302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39373132302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39383134342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39383134342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A39393136382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A39393136382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130303139322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130303139322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130313231362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130313231362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130323234302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130323234302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130333236342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130333236342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130343238382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130343238382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130353331322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130353331322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130363333362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130363333362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130373336302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130373336302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130383338342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130383338342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3130393430382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3130393430382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131303433322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131303433322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131313435362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131313435362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131323438302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131323438302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131333530342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131333530342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131343532382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131343532382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131353535322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131353535322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131363537362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131363537362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131373630302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131373630302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131383632342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131383632342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3131393634382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3131393634382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132303637322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132303637322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132313639362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132313639362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132323732302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132323732302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132333734342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132333734342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132343736382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132343736382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132353739322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132353739322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132363831362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132363831362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132373834302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132373834302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132383836342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132383836342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3132393838382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3132393838382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133303931322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133303931322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133313933362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133313933362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133323936302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133323936302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133333938342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133333938342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133353030382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133353030382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133363033322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133363033322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133373035362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133373035362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133383038302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133383038302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3133393130342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3133393130342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134303132382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134303132382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134313135322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134313135322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134323137362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134323137362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134333230302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134333230302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134343232342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134343232342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134353234382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134353234382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134363237322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134363237322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134373239362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134373239362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134383332302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134383332302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3134393334342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3134393334342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135303336382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135303336382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135313339322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135313339322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135323431362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135323431362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135333434302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135333434302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135343436342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135343436342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135353438382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135353438382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135363531322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135363531322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135373533362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135373533362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135383536302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135383536302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3135393538342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3135393538342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136303630382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136303630382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136313633322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136313633322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136323635362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136323635362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136333638302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136333638302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136343730342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136343730342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136353732382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136353732382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136363735322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136363735322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136373737362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136373737362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136383830302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136383830302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3136393832342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3136393832342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137303834382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137303834382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137313837322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137313837322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137323839362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137323839362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137333932302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137333932302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137343934342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137343934342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137353936382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137353936382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137363939322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137363939322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137383031362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137383031362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3137393034302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3137393034302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138303036342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138303036342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138313038382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138313038382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138323131322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138323131322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138333133362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138333133362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138343136302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138343136302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138353138342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138353138342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138363230382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138363230382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138373233322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138373233322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138383235362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138383235362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3138393238302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3138393238302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139303330342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139303330342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139313332382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139313332382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139323335322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139323335322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139333337362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139333337362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139343430302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139343430302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139353432342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139353432342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139363434382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139363434382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139373437322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139373437322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139383439362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139383439362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3139393532302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3139393532302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230303534342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230303534342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230313536382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230313536382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230323539322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230323539322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230333631362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230333631362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230343634302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230343634302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230353636342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230353636342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230363638382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230363638382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230373731322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230373731322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230383733362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230383733362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3230393736302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3230393736302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231303738342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231303738342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231313830382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231313830382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231323833322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231323833322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231333835362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231333835362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231343838302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231343838302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231353930342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231353930342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231363932382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231363932382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231373935322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231373935322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3231383937362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3231383937362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232303030302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232303030302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232313032342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232313032342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232323034382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232323034382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232333037322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232333037322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232343039362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232343039362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232353132302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232353132302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232363134342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232363134342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232373136382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232373136382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232383139322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232383139322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3232393231362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3232393231362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233303234302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233303234302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233313236342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233313236342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233323238382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233323238382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233333331322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233333331322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233343333362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233343333362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233353336302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233353336302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233363338342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233363338342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233373430382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233373430382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233383433322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233383433322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3233393435362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3233393435362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234303438302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234303438302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234313530342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234313530342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234323532382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234323532382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234333535322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234333535322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234343537362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234343537362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234353630302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234353630302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234363632342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234363632342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234373634382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234373634382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234383637322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234383637322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3234393639362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3234393639362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235303732302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235303732302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235313734342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235313734342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235323736382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235323736382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235333739322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235333739322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235343831362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235343831362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235353834302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235353834302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235363836342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235363836342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235373838382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235373838382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235383931322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235383931322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3235393933362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3235393933362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236303936302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236303936302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236313938342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236313938342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236333030382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236333030382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236343033322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236343033322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236353035362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236353035362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236363038302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236363038302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236373130342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236373130342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236383132382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236383132382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3236393135322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3236393135322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237303137362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237303137362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237313230302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237313230302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237323232342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237323232342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237333234382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237333234382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237343237322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237343237322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237353239362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237353239362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237363332302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237363332302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237373334342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237373334342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237383336382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237383336382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3237393339322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3237393339322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238303431362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238303431362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238313434302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238313434302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238323436342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238323436342C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238333438382C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238333438382C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238343531322C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238343531322C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238353533362C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238353533362C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238363536302C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238363536302C227265706F727465645F6475726174696F6E223A313032342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D2C7B22707473223A3238373538342C2264697363617264223A66616C73652C226465636F64655F74696D657374616D70223A3238373538342C227265706F727465645F6475726174696F6E223A3730342C2273616D706C655F636F756E74223A313032342C2273616D706C655F666F726D6174223A22666C7470222C22736B69705F73616D706C6573223A6E756C6C7D5D7D7D');
CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id),
            workflow TEXT NOT NULL DEFAULT 'generic' CHECK(workflow IN ('generic','single_source_v1'))
        ) STRICT;
INSERT INTO "state" VALUES(1,'04c85d5b-8a06-412b-b572-9a331d9ba34d',7,'generic');
CREATE INDEX revision_parent ON revisions(parent_id);
CREATE INDEX history_revision ON history(revision_id);
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
COMMIT;
