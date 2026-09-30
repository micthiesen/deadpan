use super::*;

const FIXTURES: [(&str, &str); 3] = [
    (
        "focused",
        include_str!("../fixtures/v39-render-focused.sql"),
    ),
    ("source", include_str!("../fixtures/v39-render-source.sql")),
    (
        "generated",
        include_str!("../fixtures/v39-render-generated.sql"),
    ),
];
fn fixture(root: &Path, name: &str, sql: &str) -> Result<PathBuf> {
    let package = root.join(format!("{name}.deadpan"));
    fs::create_dir(&package)?;
    for path in ["Snapshots", "Media/Originals", "Media/Generated"] {
        fs::create_dir_all(package.join(path))?;
    }
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(sql)?;
    Ok(package)
}
/// Preserve every preexisting cell, including opaque operational provenance,
/// rather than comparing parsed/reformatted documents or a subset of tables.
fn old_rows(connection: &Connection) -> Result<Vec<String>> {
    let tables=connection.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'render_%' AND name NOT LIKE 'sqlite_%' ORDER BY name")?.query_map([],|row|row.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
    let mut result = Vec::new();
    for table in tables {
        assert!(
            table
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        );
        result.push(table.clone());
        let mut statement = connection.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
        let width = statement.column_count();
        result.extend(
            statement
                .query_map([], |row| {
                    (0..width)
                        .map(|index| row.get_ref(index).map(|value| format!("{value:?}")))
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map(|values| values.join("|"))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(result)
}
#[test]
fn authentic_schema39_gains_empty_render_tables_without_rewriting_any_old_cell() -> Result {
    for (name, sql) in FIXTURES {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path(), name, sql)?;
        let database = Connection::open(package.join("project.sqlite"))?;
        let before = old_rows(&database)?;
        assert!(matches!(
            ProjectStore::open(&package, AccessMode::ReadOnly),
            Err(StoreError::MigrationRequired(39))
        ));
        let outcome = ProjectStore::migrate(&package)?;
        assert_eq!(
            (outcome.from_schema, outcome.to_schema),
            (39, DATABASE_SCHEMA_VERSION)
        );
        assert_eq!(old_rows(&database)?, before);
        let backup = Connection::open(outcome.backup.unwrap())?;
        assert_eq!(old_rows(&backup)?, before);
        assert_eq!(
            backup.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            39
        );
        for table in [
            "render_jobs",
            "render_job_heads",
            "render_attempts",
            "render_candidate_checkpoints",
        ] {
            assert_eq!(
                database.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))?,
                0
            );
        }
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        store.validate()?;
        assert_eq!(store.render_jobs(None, 256)?.len(), 0);
    }
    Ok(())
}
#[test]
fn schema39_render_vocabulary_collision_fails_before_promotion() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path(), FIXTURES[0].0, FIXTURES[0].1)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch(
        "CREATE TABLE render_jobs(foreign_value TEXT); INSERT INTO render_jobs VALUES('preserve')",
    )?;
    let before = old_rows(&database)?;
    assert!(matches!(
        ProjectStore::migrate(&package),
        Err(StoreError::MigrationFailed { .. })
    ));
    assert_eq!(old_rows(&database)?, before);
    assert_eq!(
        database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        39
    );
    assert_eq!(
        database.query_row("SELECT foreign_value FROM render_jobs", [], |row| row
            .get::<_, String>(0))?,
        "preserve"
    );
    Ok(())
}
