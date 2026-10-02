//! Retain unsupported development fixtures byte-for-byte at the cell level,
//! including render and publication history. No fixture is relabeled.
use super::*;

const SCHEMA41: &str = include_str!("../fixtures/v41-automatic-render.sql");

fn fixture(root: &Path, sql: &str) -> Result<PathBuf> {
    let package = root.join("automatic-migration.deadpan");
    fs::create_dir(&package)?;
    for directory in [
        "Snapshots",
        "Media/Originals",
        "Media/Generated",
        "Media/RenderCandidates",
    ] {
        fs::create_dir_all(package.join(directory))?;
    }
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(sql)?;
    Ok(package)
}

fn failed_without_promotion(package: &Path, version: u32) -> Result<StoreError> {
    if (39..=42).contains(&version) {
        development_break::assert_refused(package, version)?;
        return Ok(StoreError::UnsupportedSchema(version));
    }
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = development_break::cells(&database)?;
    let error = ProjectStore::migrate(package).unwrap_err();
    let StoreError::MigrationFailed { backup, source } = error else {
        panic!("expected a backed-up migration rejection: {error}");
    };
    assert_eq!(development_break::cells(&database)?, before);
    assert_eq!(
        development_break::cells(&Connection::open(backup)?)?,
        before
    );
    assert_eq!(
        database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        version
    );
    Ok(*source)
}

#[test]
fn authentic_schema41_refusal_preserves_every_cell_with_wal_reader() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path(), SCHEMA41)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "journal_mode", "WAL")?;
    let before = development_break::cells(&database)?;
    for (table, count) in [
        ("render_jobs", 3),
        ("render_attempts", 26),
        ("render_candidate_checkpoints", 2),
        ("render_publications", 12),
        ("render_publication_operations", 23),
    ] {
        assert_eq!(
            database.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))?,
            count
        );
    }
    database.execute_batch("BEGIN")?;
    assert_eq!(development_break::cells(&database)?, before);
    development_break::assert_refused(&package, 41)?;
    assert_eq!(development_break::cells(&database)?, before);
    database.execute_batch("COMMIT")?;
    assert_eq!(development_break::cells(&database)?, before);
    Ok(())
}

#[test]
fn old_databases_reject_even_empty_decision_table_collisions() -> Result {
    for (version, sql) in [
        (1, include_str!("../fixtures/v1-history.sql")),
        (39, include_str!("../fixtures/v39-render-focused.sql")),
        (40, include_str!("../fixtures/v40-publication-render.sql")),
        (41, SCHEMA41),
    ] {
        for populated in [false, true] {
            let scratch = tempfile::tempdir()?;
            let package = fixture(scratch.path(), sql)?;
            let database = Connection::open(package.join("project.sqlite"))?;
            database.execute_batch("CREATE TABLE render_encoding_decisions(foreign_value TEXT)")?;
            if populated {
                database
                    .execute_batch("INSERT INTO render_encoding_decisions VALUES('preserve')")?;
            }
            failed_without_promotion(&package, version)?;
            assert_eq!(
                database.query_row(
                    "SELECT COUNT(*) FROM render_encoding_decisions",
                    [],
                    |row| row.get::<_, i64>(0)
                )?,
                i64::from(populated)
            );
        }
    }
    Ok(())
}

fn automatic_policy() -> serde_json::Value {
    serde_json::json!({"schema_version": 1, "selection": "automatic", "algorithm": "automatic_sdr_v1"})
}

#[test]
fn schema40_and41_refuse_automatic_intent_before_parsing_it() -> Result {
    for (version, sql) in [
        (40, include_str!("../fixtures/v40-publication-render.sql")),
        (41, SCHEMA41),
    ] {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path(), sql)?;
        let database = Connection::open(package.join("project.sqlite"))?;
        let (revision, body): (String, String) = database.query_row(
            "SELECT revision_id,intent FROM render_jobs LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut value: serde_json::Value = serde_json::from_str(&body)?;
        value["schema_version"] = 2.into();
        value["job_id"] = "future-automatic-job".into();
        value["policy"] = automatic_policy();
        // Even valid current operational intent cannot override the rejected
        // database format or authorize a rewrite of its authored history.
        let current: deadpan_jobs::render::RenderIntent = serde_json::from_value(value.clone())?;
        current.validate()?;
        database.execute(
            "INSERT INTO render_jobs(job_id,revision_id,intent) VALUES(?1,?2,?3)",
            rusqlite::params![current.job_id.as_str(), revision, value.to_string()],
        )?;
        assert!(matches!(
            failed_without_promotion(&package, version)?,
            StoreError::UnsupportedSchema(found) if found == version
        ));
    }
    Ok(())
}

#[test]
fn schema41_refusal_preserves_nested_publication_intent() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path(), SCHEMA41)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let (id, body): (String, String) = database.query_row(
        "SELECT publication_id,body FROM render_publications LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut value: serde_json::Value = serde_json::from_str(&body)?;
    value["render_intent"]["schema_version"] = 2.into();
    value["render_intent"]["policy"] = automatic_policy();
    database.execute(
        "UPDATE render_publications SET body=?1 WHERE publication_id=?2",
        rusqlite::params![value.to_string(), id],
    )?;
    assert!(matches!(
        failed_without_promotion(&package, 41)?,
        StoreError::UnsupportedSchema(41)
    ));
    Ok(())
}

#[test]
fn schema41_rejects_oversized_and_new_null_evidence_without_rewriting_cells() -> Result {
    for sql in [
        "UPDATE render_jobs SET intent=json_object('oversized',printf('%09000d',0)) WHERE job_id=(SELECT job_id FROM render_jobs LIMIT 1)",
        "UPDATE render_publications SET body=json_object('oversized',printf('%0170000d',0)) WHERE publication_id=(SELECT publication_id FROM render_publications LIMIT 1)",
        "UPDATE render_attempts SET body=json_set(body,'$.encoding_decision',NULL) WHERE attempt_id=(SELECT attempt_id FROM render_attempts LIMIT 1)",
        "UPDATE render_candidate_checkpoints SET media=json_set(media,'$.encoding_decision',NULL) WHERE attempt_id=(SELECT attempt_id FROM render_candidate_checkpoints LIMIT 1)",
    ] {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path(), SCHEMA41)?;
        let database = Connection::open(package.join("project.sqlite"))?;
        database.execute_batch(sql)?;
        failed_without_promotion(&package, 41)?;
    }
    Ok(())
}

#[test]
fn authentic_old_database_rejects_injected_register_tables_without_losing_cells() -> Result {
    for table in ["register_state", "register_contents", "registers"] {
        for populated in [false, true] {
            let scratch = tempfile::tempdir()?;
            let package = fixture(scratch.path(), include_str!("../fixtures/v1-history.sql"))?;
            let database = Connection::open(package.join("project.sqlite"))?;
            database.execute_batch(&format!("CREATE TABLE {table}(foreign_value TEXT)"))?;
            if populated {
                database.execute_batch(&format!("INSERT INTO {table} VALUES('preserve')"))?;
            }
            let error = failed_without_promotion(&package, 1)?;
            assert!(
                error
                    .to_string()
                    .contains(&format!("table {table} already exists")),
                "{error}"
            );
            assert_eq!(
                database.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))?,
                i64::from(populated)
            );
        }
    }
    Ok(())
}
