use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::Connection;
use tempfile::TempDir;

pub fn fixture_sql(name: &str) -> String {
    let content = include_str!("../fixtures/create_fixtures.sql");
    let mut fixtures = HashMap::new();
    let mut current_name = None::<String>;
    let mut buffer = String::new();

    for line in content.lines() {
        if let Some(name) = line.strip_prefix("-- @fixture ") {
            if let Some(previous) = current_name.replace(name.trim().to_owned()) {
                fixtures.insert(previous, buffer.trim().to_owned());
                buffer.clear();
            }
        } else {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }

    if let Some(previous) = current_name {
        fixtures.insert(previous, buffer.trim().to_owned());
    }

    fixtures.get(name).cloned().expect("fixture exists")
}

pub fn create_db(dir: &TempDir, file_name: &str, fixture_name: &str) -> PathBuf {
    let path = dir.path().join(file_name);
    let connection = Connection::open(&path).expect("create sqlite db");
    connection
        .execute_batch(&fixture_sql(fixture_name))
        .expect("apply fixture sql");
    path
}

#[allow(dead_code)]
pub fn create_db_with_sql(dir: &TempDir, file_name: &str, sql: &str) -> PathBuf {
    let path = dir.path().join(file_name);
    let connection = Connection::open(&path).expect("create sqlite db");
    connection.execute_batch(sql).expect("apply custom sql");
    path
}
