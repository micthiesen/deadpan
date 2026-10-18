use deadpan_core::{
    CaptionPlacement, InstancePath, IterationId, MAX_DOCUMENT_DEPTH, MAX_IDENTITY_BYTES,
    MAX_SEMANTIC_PROGRAM_BYTES, RepeatEditBranch, RepeatEditStep, RepeatInstance, ScopedNodeTarget,
    SemanticZoomCenter, framing_zoom::ZoomKind,
};

use super::*;

fn wide_id(prefix: &str) -> String {
    format!("{prefix}{}", "x".repeat(MAX_IDENTITY_BYTES - prefix.len()))
}

fn authority(depth: usize, ordinal: u32) -> SemanticScopedContext {
    let pairs: Vec<_> = (0..depth)
        .map(|index| {
            (
                NodeId::new(wide_id(&format!("repeat{index}"))).unwrap(),
                IterationId {
                    allocation: RevisionId::new(wide_id(&format!("allocation{index}"))).unwrap(),
                    ordinal,
                },
            )
        })
        .collect();
    let source = NodeId::new(wide_id("source")).unwrap();
    SemanticScopedContext {
        root: pairs[0].0.clone(),
        target: ScopedNodeTarget {
            node: source.clone(),
            repeats: pairs
                .iter()
                .map(|(repeat, iteration)| RepeatEditStep {
                    repeat: repeat.clone(),
                    branch: RepeatEditBranch::Play {
                        iteration: iteration.clone(),
                    },
                })
                .collect(),
        },
        presentation: InstancePath {
            node: source,
            repeats: pairs
                .into_iter()
                .map(|(node, iteration)| RepeatInstance { node, iteration })
                .collect(),
        },
    }
}

fn request(scoped: SemanticScopedContext) -> Request {
    Request {
        protocol: PROTOCOL_VERSION,
        project_id: ProjectId::new(wide_id("project")).unwrap(),
        expected_revision: RevisionId::new(wide_id("before")).unwrap(),
        expected_bank_version: u64::MAX,
        dry_run: true,
        operation: Operation::Apply {
            program: Arc::new(
                SemanticProgram::new(vec![SemanticInstruction::Zoom {
                    kind: ZoomKind::Smash {
                        scale: deadpan_core::ExactRatio::integer(2),
                    },
                    center: SemanticZoomCenter::Keep,
                }])
                .unwrap(),
            ),
            parent: NodeId::new(wide_id("root")).unwrap(),
            cursor: ProjectFrame(i64::MAX),
            selected_child: Some(scoped.root.clone()),
            visual_selection: None,
            original: None,
            scoped: Some(scoped),
            new_revision: Some(RevisionId::new(wide_id("after")).unwrap()),
        },
    }
}

#[test]
fn macro_request_admits_maximum_depth_scoped_zoom_authority_and_bounds_raw_input() {
    // Root Sequence depth 0 + 255 one-play Repeats + a Source at depth 256.
    // Selecting ordinal zero at every one-play Repeat preserves finite source
    // duration even at the maximum legal structural depth.
    let mut request = request(authority(MAX_DOCUMENT_DEPTH - 1, 0));
    request.operation = match request.operation {
        Operation::Apply {
            program,
            parent,
            selected_child,
            scoped,
            new_revision,
            ..
        } => Operation::Apply {
            program,
            parent,
            selected_child,
            scoped,
            new_revision,
            cursor: ProjectFrame(0),
            visual_selection: None,
            original: None,
        },
        _ => unreachable!("Apply fixture"),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert!(bytes.len() > MAX_SEMANTIC_PROGRAM_BYTES + 4096);
    assert!(bytes.len() < MAX_REQUEST_BYTES);
    request.validate().unwrap();
    let parsed = Request::from_json(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        serde_json::to_value(&request).unwrap()
    );
    let mut exact_limit = bytes;
    exact_limit.resize(MAX_REQUEST_BYTES, b' ');
    Request::from_json(&exact_limit).unwrap();
    exact_limit.push(b' ');
    assert_eq!(
        Request::from_json(&exact_limit).unwrap_err().code,
        "LimitExceeded"
    );
}

#[test]
fn macro_scoped_authority_allowance_matches_widest_actual_serde_shape_for_apply_and_run() {
    // This checks the wire type's complete depth allowance. A concrete document
    // may need a smaller Repeat count because its root/leaf also consume depth.
    let scoped = authority(MAX_DOCUMENT_DEPTH, u32::MAX);
    assert_eq!(
        serde_json::to_vec(&scoped).unwrap().len(),
        MAX_SCOPED_CONTEXT_BYTES
    );
    let mut request = request(scoped);
    request.validate().unwrap();
    Request::from_json(&serde_json::to_vec(&request).unwrap()).unwrap();
    let Operation::Apply {
        parent,
        cursor,
        selected_child,
        scoped,
        new_revision,
        ..
    } = request.operation
    else {
        panic!("Apply fixture");
    };
    request.operation = Operation::Run {
        register: RegisterName::new('z').unwrap(),
        parent,
        cursor,
        selected_child,
        visual_selection: None,
        original: None,
        scoped,
        count: NonZeroU32::new(u32::MAX).unwrap(),
        new_revision,
    };
    request.validate().unwrap();
    Request::from_json(&serde_json::to_vec(&request).unwrap()).unwrap();
}

#[test]
fn macro_larger_runtime_envelope_keeps_the_program_byte_limit_independent() {
    let instruction = SemanticInstruction::SetCaption {
        text: "x".repeat(deadpan_core::MAX_CAPTION_CHARS),
        placement: CaptionPlacement::Bottom,
        delay: None,
        reveal: None,
    };
    let instructions = vec![instruction; deadpan_core::MAX_SEMANTIC_PROGRAM_INSTRUCTIONS];
    assert_eq!(
        SemanticProgram::new(instructions.clone()).unwrap_err().code,
        EditErrorCode::LimitExceeded
    );
    let mut value = serde_json::to_value(request(authority(1, 0))).unwrap();
    value["operation"]["scoped"] = Value::Null;
    value["operation"]["program"] = json!({"instructions":instructions});
    let program = serde_json::to_vec(&value["operation"]["program"]).unwrap();
    let request = serde_json::to_vec(&value).unwrap();
    assert!(program.len() > MAX_SEMANTIC_PROGRAM_BYTES);
    assert!(request.len() < MAX_REQUEST_BYTES);
    let error = Request::from_json(&request).unwrap_err();
    assert_eq!(error.code, "HostProtocolInvalid");
    assert!(error.message.contains("byte limit"), "{error}");
}
