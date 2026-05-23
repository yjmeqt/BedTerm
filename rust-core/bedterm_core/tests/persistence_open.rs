use bedterm_core::persistence::Database;

#[test]
fn open_in_memory_bootstraps_schema() {
    let db = Database::open_in_memory().expect("open");
    assert_eq!(db.schema_version().unwrap(), 1);
}

#[test]
fn open_file_then_reopen_keeps_schema_version() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    let first = Database::open(path).unwrap();
    assert_eq!(first.schema_version().unwrap(), 1);
    drop(first);
    let second = Database::open(path).unwrap();
    assert_eq!(second.schema_version().unwrap(), 1);
}
