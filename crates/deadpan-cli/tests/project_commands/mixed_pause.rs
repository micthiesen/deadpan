//! Public command protocol parity for explicit mixed Repeat pause authority.

use deadpan_core::{
    AssetId, AudioTimingId, InstancePath, IterationId, NodeKind, OccurrenceIdentities,
    OccurrencePauseIdentities, ProjectFrame, RepeatEditBranch, RepeatEditStep, RepeatInstance,
    RevisionId, ScopedNodeTarget, SplitIdentities,
};
use deadpan_plan::{Picture, RenderPlan};

use super::*;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn snapshot(package: &Path) -> Result<ProjectDocument> {
    Ok(ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?)
}

fn rows(package: &Path) -> Result<Vec<String>> {
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let mut rows = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
        "SELECT json_array(owner_revision,ordinal,step_revision,document) FROM transaction_steps ORDER BY owner_revision,ordinal",
        "SELECT json_array(singleton,version) FROM register_state",
        "SELECT json_array(name,content_id) FROM registers ORDER BY name",
    ] {
        rows.extend(
            database
                .prepare(query)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(rows)
}

fn same_content(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

fn command(
    package: &Path,
    input: &Path,
    command: Command,
    revision: &str,
) -> Result<ProjectDocument> {
    let before = snapshot(package)?;
    fs::write(input, json!({"protocol":1,"project_id":before.project_id(),"expected_revision":before.revision_id(),"new_revision":revision,"command":command}).to_string())?;
    success(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    snapshot(package)
}

fn qualified_nested_project(root: &Path, input: &Path) -> Result<PathBuf> {
    let package = create(root)?;
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()?;
    let retained = success(&[
        "project",
        "retain-original",
        package.to_str().unwrap(),
        source.to_str().unwrap(),
    ])?;
    let before = snapshot(&package)?;
    fs::write(
        input,
        json!({"protocol":1,"registration":{
        "expected_revision":before.revision_id(),"new_revision":"qualified",
        "original":retained["retained_original"]["record"]["object"]["content"],
        "new_asset_id":"original","label":"Qualified CFR Original",
        "insertion":{"parent":before.root(),"index":0,"node":"source","label":"Original"}
    },"streams":{"type":"video_only"}})
        .to_string(),
    )?;
    success(&[
        "project",
        "register-source",
        package.to_str().unwrap(),
        "--request-json",
        input.to_str().unwrap(),
    ])?;
    let registered = snapshot(&package)?;
    assert_eq!(registered.duration()?.frames(), 120);
    assert!(
        registered.assets()[&AssetId::new("original")?]
            .source_qualification
            .is_some()
    );
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?
            .source_video_index(registered.revision_id(), &AssetId::new("original")?)?
            .frames()
            .len(),
        120
    );
    for (child, repeat, plays) in [("source", "inner", 2), ("inner", "outer", 3)] {
        command(
            &package,
            input,
            Command::WrapRepeat {
                node: node(child),
                id: node(repeat),
                plays,
                gap: None,
                anchor_policy: Default::default(),
            },
            &format!("wrap-{repeat}"),
        )?;
    }
    assert_eq!(snapshot(&package)?.duration()?.frames(), 720);
    Ok(package)
}

fn iteration(document: &ProjectDocument, owner: &str, ordinal: u32) -> IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(owner)].kind else {
        panic!("Repeat")
    };
    iterations.at(ordinal).unwrap()
}

fn pause_request(document: &ProjectDocument, outer_default: bool, name: &str) -> Result<Value> {
    let choices = [("outer", 1, outer_default), ("inner", 1, !outer_default)];
    let target = ScopedNodeTarget {
        node: node("source"),
        repeats: choices
            .iter()
            .map(|(owner, ordinal, default)| RepeatEditStep {
                repeat: node(owner),
                branch: if *default {
                    RepeatEditBranch::Default
                } else {
                    RepeatEditBranch::Play {
                        iteration: iteration(document, owner, *ordinal),
                    }
                },
            })
            .collect(),
    };
    let within = InstancePath {
        node: node("source"),
        repeats: choices
            .iter()
            .map(|(owner, ordinal, _)| RepeatInstance {
                node: node(owner),
                iteration: iteration(document, owner, *ordinal),
            })
            .collect(),
    };
    // Outer play1, inner play1, seventeen frames into the measured Original.
    let at = ProjectFrame(377);
    let needs = document.pause_insertion_scoped(at, &target, &within)?;
    let command = Command::InsertOccurrenceTime {
        at,
        within: Some(within),
        target: Some(target),
        hold: HoldRecipe {
            duration: FrameDuration::new(3)?,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
        id: node(&format!("{name}-pause")),
        ai: false,
        all_plays: false,
        timing: AudioTimingId {
            allocation: RevisionId::new(name)?,
            ordinal: 0,
        },
        identities: OccurrencePauseIdentities {
            isolation: OccurrenceIdentities {
                nodes: (0..needs.isolation_nodes)
                    .map(|n| node(&format!("{name}-isolation-{n}")))
                    .collect(),
                marks: (0..needs.isolation_marks)
                    .map(|n| deadpan_core::MarkId::new(format!("{name}-mark-{n}")).unwrap())
                    .collect(),
            },
            split: SplitIdentities {
                nodes: (0..needs.split_nodes)
                    .map(|n| node(&format!("{name}-split-{n}")))
                    .collect(),
            },
            container: needs.container.then(|| node(&format!("{name}-container"))),
            gap: needs.gap.then(|| node(&format!("{name}-gap"))),
        },
    };
    Ok(
        json!({"protocol":1,"project_id":document.project_id(),"expected_revision":document.revision_id(),"new_revision":name,"command":command}),
    )
}

fn assert_preserved_source(before: &ProjectDocument, after: &ProjectDocument, old_cuts: &[i64]) {
    let old_plan = RenderPlan::compile(before).unwrap();
    let new_plan = RenderPlan::compile(after).unwrap();
    for old_frame in 0..720 {
        let shift =
            i64::try_from(old_cuts.iter().filter(|cut| **cut <= old_frame).count()).unwrap() * 3;
        let old = old_plan.picture(ProjectFrame(old_frame)).unwrap();
        let new = new_plan.picture(ProjectFrame(old_frame + shift)).unwrap();
        let Picture::Source {
            asset: old_asset,
            point: old_point,
            ..
        } = old.picture
        else {
            panic!("Original provider")
        };
        let Picture::Source {
            asset: new_asset,
            point: new_point,
            ..
        } = new.picture
        else {
            panic!("retained Original provider at {old_frame}")
        };
        assert_eq!(new_asset, old_asset, "asset at {old_frame}");
        assert_eq!(new_point, old_point, "exact Original clock at {old_frame}");
    }
}

#[test]
fn headless_mixed_pause_dry_run_commit_refusals_and_reopened_history_preserve_qualified_source()
-> Result {
    for (outer_default, name, delta, old_cuts) in [
        (true, "default-play", 9, vec![137, 377, 617]),
        (false, "play-default", 6, vec![257, 377]),
    ] {
        let scratch = tempfile::tempdir()?;
        let input = scratch.path().join("mixed-pause.json");
        let package = qualified_nested_project(scratch.path(), &input)?;
        let path = package.to_str().unwrap();
        let file = input.to_str().unwrap();
        let before = snapshot(&package)?;
        let request = pause_request(&before, outer_default, name)?;
        let initial_rows = rows(&package)?;
        for case in 0..2 {
            let mut invalid = request.clone();
            invalid["new_revision"] = json!(format!("invalid-{case}"));
            invalid["command"]["timing"]["allocation"] = invalid["new_revision"].clone();
            if case == 0 {
                invalid["command"]["all_plays"] = json!(true);
            } else {
                invalid["command"]["within"]["node"] = json!("inner");
            }
            fs::write(&input, invalid.to_string())?;
            for dry_run in [false, true] {
                let mut arguments = vec!["command", path, "--json", file];
                if dry_run {
                    arguments.push("--dry-run");
                }
                assert!(
                    !cli(&arguments)?.status.success(),
                    "invalid authority case{case}"
                );
                assert_eq!(rows(&package)?, initial_rows);
                assert_eq!(snapshot(&package)?, before);
            }
        }
        fs::write(&input, request.to_string())?;
        let preview = success(&["command", path, "--json", file, "--dry-run"])?;
        assert_eq!(preview["edit"]["duration_delta"], delta);
        assert_eq!(rows(&package)?, initial_rows);
        assert_eq!(snapshot(&package)?, before);
        let committed = success(&["command", path, "--json", file])?;
        assert_eq!(committed["outcome"]["edit"], preview["edit"]);
        let after = snapshot(&package)?;
        assert_eq!(after.duration()?.frames(), 720 + delta);
        assert_eq!(after.assets(), before.assets());
        assert_preserved_source(&before, &after, &old_cuts);
        for (ordinal, old_cut) in old_cuts.iter().enumerate() {
            let start = old_cut + i64::try_from(ordinal)? * 3;
            for frame in start..start + 3 {
                let report = success(&["inspect-plan", path, "--frame", &frame.to_string()])?;
                assert_eq!(report["sample"]["picture"]["type"], "background");
                assert_eq!(
                    report["sample"]["instance"]["node"],
                    format!("{name}-pause")
                );
            }
            let report = success(&["inspect-plan", path, "--frame", &(start + 3).to_string()])?;
            assert_eq!(report["sample"]["picture"]["type"], "source");
        }
        let committed_rows = rows(&package)?;
        assert!(
            !cli(&["command", path, "--json", file])?.status.success(),
            "stale request cannot insert twice"
        );
        assert_eq!(rows(&package)?, committed_rows);
        assert_eq!(snapshot(&package)?, after);
        success(&["project", "undo", path, "--expected", name])?;
        let undone = snapshot(&package)?;
        same_content(&undone, &before)?;
        success(&[
            "project",
            "redo",
            path,
            "--expected",
            undone.revision_id().as_str(),
        ])?;
        let redone = snapshot(&package)?;
        same_content(&redone, &after)?;
        assert_preserved_source(&before, &redone, &old_cuts);
        ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    }
    Ok(())
}
