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
#[test]
fn authentic_schema40_is_refused_without_rewriting_cells_or_creating_backups() -> Result {
    let root = tempfile::tempdir()?;
    let package = fixture(root.path())?;
    development_break::assert_refused(&package, 40)?;
    Ok(())
}
#[test]
fn schema40_refusal_does_not_inspect_or_modify_publication_vocabulary() -> Result {
    let root = tempfile::tempdir()?;
    let package = fixture(root.path())?;
    let db = Connection::open(package.join("project.sqlite"))?;
    db.execute_batch("CREATE TABLE render_publications(foreign_value TEXT); INSERT INTO render_publications VALUES('preserve')")?;
    development_break::assert_refused(&package, 40)?;
    assert_eq!(
        db.query_row("SELECT foreign_value FROM render_publications", [], |r| {
            r.get::<_, String>(0)
        })?,
        "preserve"
    );
    Ok(())
}
