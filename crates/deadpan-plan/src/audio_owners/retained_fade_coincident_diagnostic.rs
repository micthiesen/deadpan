//! Two checked edits create a marked Partition; ordinary output remains independent.
use super::*;
use deadpan_core::{
    AudioEditorialEdges, Command, CommandRequest, FrameDuration, NodeId, ProjectDocument,
    RevisionId, SplitIdentities, apply,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

thread_local! {
    static COLLECT: Cell<bool> = const { Cell::new(false) };
    static MERGES: RefCell<std::collections::BTreeMap<String, usize>> =
        const { RefCell::new(std::collections::BTreeMap::new()) };
    static BOTH_NONEMPTY: Cell<usize> = const { Cell::new(0) };
}
pub(super) fn record(event: Event) {
    if !COLLECT.get() {
        return;
    }
    if matches!(
        event.phase,
        Common::OriginMergeBefore | Common::OriginMergeReady | Common::OriginMergeAfter
    ) {
        let key = format!(
            "{:?}/old={}/new={}",
            event.phase, event.old_len, event.new_len
        );
        MERGES.with(|counts| *counts.borrow_mut().entry(key).or_default() += 1);
    }
    if event.phase == Common::OriginMergeReady && event.old_len > 0 && event.new_len > 0 {
        BOTH_NONEMPTY.set(BOTH_NONEMPTY.get() + 1);
    }
}
pub(in crate::plan) fn ordinary_case() -> Value {
    let supplement: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-query-supplement-v1/snapshot.json"
    ))
    .unwrap();
    let original = supplement["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["fixture"]["name"] == "retained_supplement/captured_marked_repeat_fragment"
        })
        .expect("frozen genuine checked bound repeat fragment");
    let before = ProjectDocument::from_json(&original["fixture"]["document"].to_string()).unwrap();
    let split = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("retained-coincident-split-loop").unwrap(),
        command: Command::Split {
            node: NodeId::new("loop").unwrap(),
            at: FrameDuration::new(4).unwrap(),
            identities: SplitIdentities {
                nodes: [
                    "coinc-left",
                    "coinc-right",
                    "coinc-loop",
                    "coinc-fragment",
                    "coinc-voice",
                ]
                .into_iter()
                .map(|id| NodeId::new(id).unwrap())
                .collect(),
            },
        },
    };
    let partitioned = apply(&before, &split)
        .unwrap()
        .forward
        .apply(&before)
        .unwrap();
    let request = CommandRequest {
        project_id: partitioned.project_id().clone(),
        expected_revision: partitioned.revision_id().clone(),
        new_revision: RevisionId::new("retained-coincident-mark-right").unwrap(),
        command: Command::SetEditorialEdges {
            node: NodeId::new("coinc-right").unwrap(),
            edges: AudioEditorialEdges {
                start: false,
                end: true,
            },
        },
    };
    let after = apply(&partitioned, &request)
        .unwrap()
        .forward
        .apply(&partitioned)
        .unwrap();
    let plan = RenderPlan::compile(&after).unwrap();
    let owners = plan
        .audio_gain_owners(AudioSample(6400)..AudioSample(6401), Default::default())
        .unwrap();
    let owner = owners
        .spans()
        .iter()
        .flat_map(|span| span.owners())
        .find(|owner| {
            owner.instance().node == NodeId::new("coinc-voice").unwrap()
                && owner.kind() == crate::AudioOwnerKind::Node
        })
        .expect("actual second-play voice owner");
    let input = plan
        .audio_owner_input(owner, AudioSample(6400), Default::default())
        .unwrap();
    let descriptor = json!({
        "name":"retained_coincident/checked_split_then_marked_partition",
        "source":"unchanged frozen captured_marked_repeat_fragment plus actual Split(loop,4) and SetEditorialEdges(coinc-right) commands",
        "frozen_source_case":original["fixture"],
        "before_document":before,"split_request":split,"partitioned_document":partitioned,
        "mark_request":request,"after_document":after,
        "selection":{"node":"coinc-voice","kind":"Node","observed_at":6400},
        "target":"right Partition end and copied marked fragment end both exact frame 8 / sample 12800, inside retained voice extent 2..10",
    });
    super::super::one(
        &descriptor,
        &input,
        &[
            ("coincident_loop_end", 12799, 12801),
            ("second_play", 6400, 12800),
        ],
    )
}

pub(in crate::plan) fn frozen_case() -> Value {
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-coincident-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(
        expected["schema"],
        "literal-ordinary-retained-coincidence-v1"
    );
    expected["case"].clone()
}

#[test]
fn literal_ordinary_retained_coincidence_matches_frozen_v1() {
    assert_eq!(ordinary_case(), frozen_case());
}
fn trace<T>(action: impl FnOnce() -> T) -> (T, Value) {
    assert!(!COLLECT.replace(true));
    assert!(!ENABLED.replace(true));
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            COLLECT.set(false);
            ENABLED.set(false);
        }
    }
    let _restore = Restore;
    MERGES.with(|counts| counts.borrow_mut().clear());
    BOTH_NONEMPTY.set(0);
    HANDLES.set(0);
    COVERAGE.with(|counts| *counts.borrow_mut() = Coverage::new());
    let result = action();
    let summary = json!({
        "actual_handles":HANDLES.get(),
        "origin_merge_events":MERGES.with(|counts| counts.borrow().clone()),
        "both_nonempty_ready":BOTH_NONEMPTY.get(),
        "coverage_debug":COVERAGE.with(|counts| format!("{:?}",counts.borrow())),
    });
    (result, summary)
}
#[test]
fn checked_split_partition_end_reaches_both_nonempty_retained_origin_merge() {
    let (body, summary) = trace(ordinary_case);
    eprintln!("retained checked coincidence diagnostic: {summary}");
    assert_eq!(body["handles"].as_array().unwrap().len(), 4);
    for handle in body["handles"].as_array().unwrap() {
        assert_eq!(handle["result"], "Some", "actual {} setup", handle["mode"]);
    }
    assert_eq!(summary["actual_handles"], 4);
    assert!(
        summary["both_nonempty_ready"].as_u64().unwrap() > 0,
        "genuine checked composition must reach the real shared merge phase with both origin arrays nonempty"
    );
}
#[test]
#[ignore = "bounded diagnostic of existing immutable corpora; no body writes or oracle refresh"]
fn existing_retained_corpora_report_actual_origin_merge_shapes() {
    let (_, summary) = trace(|| {
        let original: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/retained-fade-query-baseline-v1/snapshot.json"
        ))
        .unwrap();
        assert_eq!(matrix(), original);
        let supplement: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/retained-fade-query-supplement-v1/snapshot.json"
        ))
        .unwrap();
        assert_eq!(super::super::supplement::supplement_matrix(), supplement);
    });
    eprintln!("existing retained merge shapes: {summary}");
    assert_eq!(summary["actual_handles"], 50);
}
#[test]
#[ignore = "manual additive current-tree literal ordinary witness; explicit fresh output and parent source pins"]
fn capture_current_literal_ordinary_retained_coincidence_v1() {
    assert!(
        !ENABLED.get(),
        "capture must invoke only the literal ordinary fade path"
    );
    let output =
        std::env::var("DEADPAN_RETAINED_COINCIDENT_OUTPUT").expect("explicit fresh output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_RETAINED_COINCIDENT_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_RETAINED_COINCIDENT_SOURCE_SHA256").unwrap();
    let ordinary_source =
        std::env::var("DEADPAN_RETAINED_COINCIDENT_ORDINARY_SOURCE_SHA256").unwrap();
    for digest in [&source, &ordinary_source] {
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    let checkpoint = std::env::var("DEADPAN_RETAINED_COINCIDENT_PUBLISHED_CHECKPOINT").unwrap();
    assert_eq!(checkpoint, "ee9742711868fde9a78bcb945adfd36e67f504d6");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let body = json!({
        "schema":"literal-ordinary-retained-coincidence-v1",
        "scope":"current-tree unchanged ordinary algorithm, independently pinned against published checkpoint; NOT a pre-implementation native build",
        "variant":variant,"source_sha256":source,"ordinary_source_sha256":ordinary_source,
        "published_checkpoint":checkpoint,"case":ordinary_case(),
    });
    serde_json::to_writer_pretty(&mut file, &body).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}

#[test]
#[ignore = "manual complete external ordinary coincidence regression; independent captured body required"]
fn literal_ordinary_retained_coincidence_matches_external_v1() {
    let path = std::env::var("DEADPAN_RETAINED_COINCIDENT_EXPECTED")
        .expect("independent complete ordinary coincidence body");
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        expected["schema"],
        "literal-ordinary-retained-coincidence-v1"
    );
    assert_eq!(ordinary_case(), expected["case"]);
}
