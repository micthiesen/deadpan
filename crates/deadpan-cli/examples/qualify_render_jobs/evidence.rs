use std::{
    fs::File,
    io::{self, Seek, SeekFrom, Write},
    path::Path,
};

use deadpan_cli::encoded_render::EncodedProgress;
use deadpan_core::{
    Command, CommandRequest, ExactRatio, FrameRange, Framing, FramingPose, NodeId, ProjectDocument,
    ProjectFrame, RevisionId,
};
use deadpan_plan::{Picture, RenderPlan};
use deadpan_store::ProjectStore;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::Result;

pub(super) fn check(report: &mut Value, label: &str, passed: bool, actual: Value) -> Result {
    report["checks"]
        .as_array_mut()
        .ok_or("missing checks")?
        .push(json!({"label": label, "passed": passed, "actual": actual}));
    if !passed {
        return Err(format!("qualification failed: {label}").into());
    }
    Ok(())
}

pub(super) fn save(file: &mut File, report: &Value) -> Result {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > (8 * 1024 * 1024_usize).saturating_sub(self.0.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "qualification report byte bound",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut encoded = Bounded(Vec::new());
    serde_json::to_writer_pretty(&mut encoded, report)?;
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    file.write_all(&encoded.0)?;
    file.sync_all()?;
    Ok(())
}

/// Exact authoring cells, including all historical JSON/patch text. Separate
/// operational render tables are intentionally outside this evidence digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct Authoring {
    sha256: String,
    bytes: u64,
    table_rows: Vec<u64>,
}

pub(super) fn authored(package: &Path) -> Result<Authoring> {
    let mut connection = Connection::open_with_flags(
        package.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let transaction = connection.transaction()?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut table_rows = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        hasher.update(query.as_bytes());
        let mut statement = transaction.prepare(query)?;
        let mut rows = statement.query([])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            let length = u64::try_from(value.len())?;
            bytes = bytes
                .checked_add(length)
                .ok_or("authored byte count overflow")?;
            count += 1;
            if bytes > 16 * 1024 * 1024 || count > 4096 {
                return Err("qualification authoring evidence exceeds fixture bound".into());
            }
            hasher.update(length.to_le_bytes());
            hasher.update(value.as_bytes());
        }
        table_rows.push(count);
    }
    let sha256 = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(Authoring {
        sha256,
        bytes,
        table_rows,
    })
}

pub(super) fn same_content(before: &ProjectDocument, after: &ProjectDocument) -> Result<bool> {
    let mut before = serde_json::to_value(before)?;
    let mut after = serde_json::to_value(after)?;
    before
        .as_object_mut()
        .ok_or("document must be an object")?
        .remove("revision_id")
        .ok_or("missing revision")?;
    after
        .as_object_mut()
        .ok_or("document must be an object")?
        .remove("revision_id")
        .ok_or("missing revision")?;
    Ok(before == after)
}

pub(super) fn mutate(
    store: &mut ProjectStore,
    update: EncodedProgress,
    prefix: &str,
) -> Result<Value> {
    let original = store.snapshot()?;
    let edit = RevisionId::new(format!("{prefix}-live-edit"))?;
    let undo = RevisionId::new(format!("{prefix}-live-undo"))?;
    let redo = RevisionId::new(format!("{prefix}-live-redo"))?;
    let restored = RevisionId::new(format!("{prefix}-live-restored"))?;
    store.commit(&CommandRequest {
        project_id: original.project_id().clone(),
        expected_revision: original.revision_id().clone(),
        new_revision: edit.clone(),
        command: Command::SetFraming {
            node: NodeId::new("source")?,
            framing: Some(Framing::static_pose(FramingPose::new(
                ExactRatio::new(1, 3)?,
                ExactRatio::new(2, 3)?,
                ExactRatio::new(3, 2)?,
            )?)?),
        },
    })?;
    store.undo(&edit, undo.clone())?;
    store.redo(&undo, redo.clone())?;
    store.undo(&redo, restored.clone())?;
    Ok(
        json!({"entry_revision": original.revision_id(), "edit": edit, "undo": undo,
        "redo": redo, "restored_revision": restored, "completed_frames": update.completed_frames,
        "total_frames": update.total_frames, "restoration_excludes_revision_identity": true}),
    )
}

pub(super) fn coverage(document: &ProjectDocument, range: FrameRange) -> Result<Value> {
    let plan = RenderPlan::compile(document)?;
    let mut source = 0_u64;
    let mut freeze = 0_u64;
    let mut generated = 0_u64;
    let mut background = 0_u64;
    for frame in range.start().0..range.end().0 {
        match plan.picture(ProjectFrame(frame))?.picture {
            Picture::Source { .. } => source += 1,
            Picture::Freeze { .. } => freeze += 1,
            Picture::Accepted {
                generated: Some(_), ..
            } => generated += 1,
            Picture::Background | Picture::Blank => background += 1,
            Picture::Still { .. }
            | Picture::Accepted {
                generated: None, ..
            } => return Err("fixture contains an unqualified picture provider".into()),
        }
    }
    Ok(
        json!({"source": source, "freeze": freeze, "original": source + freeze,
        "generated": generated, "background": background,
        "scope": "indexed committed picture providers; native encode separately admits each sampled provider"}),
    )
}
