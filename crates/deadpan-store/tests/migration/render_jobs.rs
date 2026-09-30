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
#[test]
fn authentic_schema39_is_refused_without_rewriting_cells_or_creating_backups() -> Result {
    for (name, sql) in FIXTURES {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path(), name, sql)?;
        development_break::assert_refused(&package, 39)?;
    }
    Ok(())
}
#[test]
fn schema39_refusal_does_not_inspect_or_modify_render_vocabulary() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path(), FIXTURES[0].0, FIXTURES[0].1)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch(
        "CREATE TABLE render_jobs(foreign_value TEXT); INSERT INTO render_jobs VALUES('preserve')",
    )?;
    development_break::assert_refused(&package, 39)?;
    assert_eq!(
        database.query_row("SELECT foreign_value FROM render_jobs", [], |row| row
            .get::<_, String>(0))?,
        "preserve"
    );
    Ok(())
}
