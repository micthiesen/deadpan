//! Current-format durability and supported additive migration checks.
//! Obsolete development formats are refused without opening a writer.
use deadpan_core::{NodeId, ProjectDocument, RevisionId};
use deadpan_store::{AccessMode, DATABASE_SCHEMA_VERSION, ProjectStore, StoreError};
use rusqlite::Connection;
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};
type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

#[path = "migration/audio_gap_bindings.rs"]
mod audio_gap_bindings;
#[path = "migration/audio_reanchors.rs"]
mod audio_reanchors;
#[path = "migration/automatic_render.rs"]
mod automatic_render;
#[path = "migration/current_authoring.rs"]
mod current_authoring;
#[path = "migration/development_break.rs"]
mod development_break;
#[path = "migration/framing.rs"]
mod framing;
#[path = "migration/gain.rs"]
mod gain;
#[path = "migration/insert_time.rs"]
mod insert_time;
#[path = "migration/picture_context.rs"]
mod picture_context;
#[path = "migration/publication.rs"]
mod publication;
#[path = "migration/render_jobs.rs"]
mod render_jobs;
#[path = "migration/retime.rs"]
mod retime;
#[path = "migration/sequence_insert.rs"]
mod sequence_insert;
#[path = "migration/sound_routes.rs"]
mod sound_routes;
#[path = "migration/source_selection.rs"]
mod source_selection;

fn docs(connection: &Connection) -> Result<Vec<(String, String)>> {
    Ok(connection
        .prepare("SELECT id, document FROM revisions ORDER BY id")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}

fn metadata(connection: &Connection) -> Result<String> {
    let mut parts = Vec::new();
    for sql in [
        "SELECT json_array(id,parent_id,kind) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        parts.extend(
            connection
                .prepare(sql)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(parts.join("\n"))
}

fn history_json(connection: &Connection) -> Result<Vec<(String, String)>> {
    Ok(connection
        .prepare("SELECT request,edit FROM history ORDER BY id")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}

fn contents(connection: &Connection) -> Result<String> {
    let mut values = vec![
        metadata(connection)?,
        format!(
            "{}",
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?
        ),
    ];
    values.extend(docs(connection)?.into_iter().map(|(_, json)| json));
    values.extend(
        connection
            .prepare("SELECT json_array(request,edit) FROM history ORDER BY id")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    );
    Ok(values.join("\n"))
}

/// Fixed current-schema snapshots preserve the authored regression scenarios
/// independently of obsolete history parsers. Qualified fixtures retain their
/// measured receipt and original bytes; no qualification is invented here.
fn current_fixture(root: &Path, json: &str, media_sql: Option<&str>) -> Result<PathBuf> {
    let document = ProjectDocument::from_json(json)?;
    let package = root.join("current.deadpan");
    if let Some(media_sql) = media_sql {
        let empty = ProjectDocument::new(
            document.project_id().clone(),
            document.revision_id().clone(),
            document.presentation_basis().clone(),
            document.root().clone(),
        )?;
        drop(ProjectStore::create(&package, &empty)?);
        fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native/deadpan-source/tests/fixtures/offset-bframes.mp4"), package.join("Media/Originals/blake3-2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f"))?;
        let database = Connection::open(package.join("project.sqlite"))?;
        database.execute_batch(media_sql)?;
        database.execute(
            "UPDATE revisions SET document=?1 WHERE id=?2",
            rusqlite::params![document.to_json()?, document.revision_id().as_str()],
        )?;
    } else {
        drop(ProjectStore::create(&package, &document)?);
    }
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    Ok(package)
}

fn pending_rename(package: &Path, node: NodeId, label: &str) -> Result {
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let document = store.snapshot()?;
    store.commit(&deadpan_core::CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("fixture-rename")?,
        command: deadpan_core::Command::Rename {
            node,
            label: label.into(),
        },
    })?;
    store.undo(
        &RevisionId::new("fixture-rename")?,
        RevisionId::new("fixture-undo")?,
    )?;
    Ok(())
}
