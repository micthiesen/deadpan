use super::*;
fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("publication.deadpan");
    fs::create_dir(&package)?;
    for path in [
        "Snapshots",
        "Media/Originals",
        "Media/Generated",
        "Media/RenderCandidates",
    ] {
        fs::create_dir_all(package.join(path))?;
    }
    let db = Connection::open(package.join("project.sqlite"))?;
    db.pragma_update(None, "foreign_keys", false)?;
    db.execute_batch(include_str!("../fixtures/v40-publication-render.sql"))?;
    Ok(package)
}
fn old_cells(db: &Connection) -> Result<Vec<String>> {
    let tables=db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'render_publication%' AND name!='render_encoding_decisions' AND name NOT LIKE 'sqlite_%' ORDER BY name")?.query_map([],|row|row.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
    let mut cells = Vec::new();
    for table in tables {
        assert!(
            table
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || v == b'_')
        );
        cells.push(table.clone());
        let mut query = db.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
        let width = query.column_count();
        cells.extend(
            query
                .query_map([], |row| {
                    (0..width)
                        .map(|i| row.get_ref(i).map(|v| format!("{v:?}")))
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map(|v| v.join("|"))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(cells)
}
#[test]
fn authentic_schema40_publication_upgrade_preserves_every_previous_cell() -> Result {
    let root = tempfile::tempdir()?;
    let package = fixture(root.path())?;
    let db = Connection::open(package.join("project.sqlite"))?;
    let before = old_cells(&db)?;
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(40))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (40, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(old_cells(&db)?, before);
    let backup = Connection::open(outcome.backup.unwrap())?;
    assert_eq!(old_cells(&backup)?, before);
    assert_eq!(
        backup.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?,
        40
    );
    for table in [
        "render_publications",
        "render_publication_operations",
        "render_encoding_decisions",
    ] {
        assert_eq!(
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))?,
            0
        );
    }
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    store.validate()?;
    assert!(store.render_publications(None, 256)?.is_empty());
    Ok(())
}
#[test]
fn schema40_publication_vocabulary_collision_preserves_original() -> Result {
    let root = tempfile::tempdir()?;
    let package = fixture(root.path())?;
    let db = Connection::open(package.join("project.sqlite"))?;
    db.execute_batch("CREATE TABLE render_publications(foreign_value TEXT); INSERT INTO render_publications VALUES('preserve')")?;
    let before = old_cells(&db)?;
    assert!(matches!(
        ProjectStore::migrate(&package),
        Err(StoreError::MigrationFailed { .. })
    ));
    assert_eq!(old_cells(&db)?, before);
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?,
        40
    );
    assert_eq!(
        db.query_row("SELECT foreign_value FROM render_publications", [], |r| {
            r.get::<_, String>(0)
        })?,
        "preserve"
    );
    Ok(())
}
