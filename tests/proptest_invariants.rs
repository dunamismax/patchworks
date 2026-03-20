use std::collections::{BTreeMap, BTreeSet};

use patchworks::db::differ::diff_databases;
use patchworks::db::inspector::{inspect_database, read_table_page};
use patchworks::db::types::{ColumnInfo, DatabaseSummary, SqlValue, TableInfo, TableQuery};
use patchworks::diff::schema::diff_schema;
use proptest::collection::{btree_map, vec};
use proptest::option;
use proptest::prelude::*;
use rusqlite::{params, Connection};
use tempfile::TempDir;

#[derive(Clone, Debug, PartialEq)]
struct PropRow {
    label: Option<String>,
    quantity: i32,
    payload: Option<Vec<u8>>,
    status: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum SchemaVariant {
    Base,
    WithStatus,
}

#[derive(Clone, Debug)]
struct ColumnShape {
    col_type: String,
    nullable: bool,
    default_value: Option<String>,
    is_primary_key: bool,
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn schema_diff_classifies_table_relationships(
        left in database_summary_strategy(),
        right in database_summary_strategy(),
    ) {
        let forward = diff_schema(&left, &right);
        let reverse = diff_schema(&right, &left);

        let left_tables = left
            .tables
            .iter()
            .map(|table| (table.name.clone(), table))
            .collect::<BTreeMap<_, _>>();
        let right_tables = right
            .tables
            .iter()
            .map(|table| (table.name.clone(), table))
            .collect::<BTreeMap<_, _>>();

        let expected_added = right_tables
            .keys()
            .filter(|name| !left_tables.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>();
        let expected_removed = left_tables
            .keys()
            .filter(|name| !right_tables.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>();
        let expected_modified = left_tables
            .iter()
            .filter_map(|(name, left_table)| {
                right_tables
                    .get(name)
                    .filter(|right_table| left_table.columns != right_table.columns)
                    .map(|_| name.clone())
            })
            .collect::<Vec<_>>();
        let expected_unchanged = left_tables
            .iter()
            .filter_map(|(name, left_table)| {
                right_tables
                    .get(name)
                    .filter(|right_table| left_table.columns == right_table.columns)
                    .map(|_| name.clone())
            })
            .collect::<Vec<_>>();

        prop_assert_eq!(&table_names(&forward.added_tables), &expected_added);
        prop_assert_eq!(&table_names(&forward.removed_tables), &expected_removed);
        prop_assert_eq!(&modified_table_names(&forward), &expected_modified);
        prop_assert_eq!(&forward.unchanged_tables, &expected_unchanged);

        prop_assert_eq!(&table_names(&reverse.added_tables), &expected_removed);
        prop_assert_eq!(&table_names(&reverse.removed_tables), &expected_added);
        prop_assert_eq!(&modified_table_names(&reverse), &expected_modified);
        prop_assert_eq!(&reverse.unchanged_tables, &expected_unchanged);
    }

    #[test]
    fn row_diff_stats_match_generated_rows(
        schema in schema_variant_strategy(),
        left_rows in row_map_strategy(),
        right_rows in row_map_strategy(),
    ) {
        let temp_dir = TempDir::new().expect("temp dir");
        let left_path = temp_dir.path().join("left.sqlite");
        let right_path = temp_dir.path().join("right.sqlite");

        create_items_db(&left_path, schema, &left_rows);
        create_items_db(&right_path, schema, &right_rows);

        let diff = diff_databases(&left_path, &right_path).expect("compute diff");
        let items = diff
            .data_diffs
            .iter()
            .find(|table| table.table_name == "items")
            .expect("items diff");

        let added_ids = items
            .added_rows
            .iter()
            .map(|row| row_id(&row[0]))
            .collect::<BTreeSet<_>>();
        let removed_ids = items
            .removed_row_keys
            .iter()
            .map(|row| row_id(&row[0]))
            .collect::<BTreeSet<_>>();
        let modified_ids = items
            .modified_rows
            .iter()
            .map(|row| row_id(&row.primary_key[0]))
            .collect::<BTreeSet<_>>();

        let expected_added = right_rows
            .keys()
            .filter(|id| !left_rows.contains_key(*id))
            .copied()
            .collect::<BTreeSet<_>>();
        let expected_removed = left_rows
            .keys()
            .filter(|id| !right_rows.contains_key(*id))
            .copied()
            .collect::<BTreeSet<_>>();
        let expected_modified = left_rows
            .iter()
            .filter_map(|(id, left_row)| {
                right_rows
                    .get(id)
                    .filter(|right_row| normalized_row(schema, left_row) != normalized_row(schema, right_row))
                    .map(|_| *id)
            })
            .collect::<BTreeSet<_>>();
        let unchanged_count = left_rows
            .iter()
            .filter_map(|(id, left_row)| {
                right_rows
                    .get(id)
                    .filter(|right_row| normalized_row(schema, left_row) == normalized_row(schema, right_row))
                    .map(|_| id)
            })
            .count() as u64;

        prop_assert!(items.warnings.is_empty());
        prop_assert_eq!(items.stats.total_rows_left, left_rows.len() as u64);
        prop_assert_eq!(items.stats.total_rows_right, right_rows.len() as u64);
        prop_assert_eq!(items.stats.added, expected_added.len() as u64);
        prop_assert_eq!(items.stats.removed, expected_removed.len() as u64);
        prop_assert_eq!(items.stats.modified, expected_modified.len() as u64);
        prop_assert_eq!(items.stats.unchanged, unchanged_count);
        prop_assert_eq!(items.stats.added + items.stats.modified + items.stats.unchanged, right_rows.len() as u64);
        prop_assert_eq!(items.stats.removed + items.stats.modified + items.stats.unchanged, left_rows.len() as u64);
        prop_assert_eq!(&added_ids, &expected_added);
        prop_assert_eq!(&removed_ids, &expected_removed);
        prop_assert_eq!(&modified_ids, &expected_modified);
    }

    #[test]
    fn sql_patch_generation_recreates_the_right_database(
        left_schema in schema_variant_strategy(),
        right_schema in schema_variant_strategy(),
        left_rows in row_map_strategy(),
        right_rows in row_map_strategy(),
    ) {
        let temp_dir = TempDir::new().expect("temp dir");
        let left_path = temp_dir.path().join("left.sqlite");
        let right_path = temp_dir.path().join("right.sqlite");
        let generated_path = temp_dir.path().join("generated.sqlite");

        create_items_db(&left_path, left_schema, &left_rows);
        create_items_db(&right_path, right_schema, &right_rows);
        std::fs::copy(&left_path, &generated_path).expect("copy left db");

        let diff = diff_databases(&left_path, &right_path).expect("compute diff");
        let generated = Connection::open(&generated_path).expect("open generated db");
        generated
            .execute_batch(&diff.sql_export)
            .expect("apply generated sql");

        let generated_summary = inspect_database(&generated_path).expect("inspect generated");
        let right_summary = inspect_database(&right_path).expect("inspect right");
        let generated_page = read_table_page(&generated_path, "items", &TableQuery::default())
            .expect("read generated page");
        let right_page =
            read_table_page(&right_path, "items", &TableQuery::default()).expect("read right page");

        prop_assert_eq!(generated_summary.tables, right_summary.tables);
        prop_assert_eq!(generated_page.rows, right_page.rows);
    }
}

fn table_names(tables: &[TableInfo]) -> Vec<String> {
    tables.iter().map(|table| table.name.clone()).collect()
}

fn modified_table_names(summary: &patchworks::db::types::SchemaDiff) -> Vec<String> {
    summary
        .modified_tables
        .iter()
        .map(|table| table.table_name.clone())
        .collect()
}

fn row_id(value: &SqlValue) -> i64 {
    match value {
        SqlValue::Integer(value) => *value,
        other => panic!("expected integer primary key, got {other:?}"),
    }
}

fn create_items_db(path: &std::path::Path, schema: SchemaVariant, rows: &BTreeMap<i64, PropRow>) {
    let mut connection = Connection::open(path).expect("open property test db");
    connection
        .execute_batch(schema.create_table_sql())
        .expect("create property test schema");

    let transaction = connection.transaction().expect("transaction");
    match schema {
        SchemaVariant::Base => {
            let mut statement = transaction
                .prepare("INSERT INTO items (id, label, quantity, payload) VALUES (?1, ?2, ?3, ?4)")
                .expect("prepare base insert");
            for (id, row) in rows {
                statement
                    .execute(params![id, row.label, row.quantity, row.payload])
                    .expect("insert base row");
            }
        }
        SchemaVariant::WithStatus => {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO items (id, label, quantity, payload, status) VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .expect("prepare status insert");
            for (id, row) in rows {
                statement
                    .execute(params![
                        id,
                        row.label,
                        row.quantity,
                        row.payload,
                        row.status
                    ])
                    .expect("insert status row");
            }
        }
    }
    transaction.commit().expect("commit property test rows");
}

fn normalized_row(schema: SchemaVariant, row: &PropRow) -> PropRow {
    let mut normalized = row.clone();
    if matches!(schema, SchemaVariant::Base) {
        normalized.status = None;
    }
    normalized
}

fn schema_variant_strategy() -> impl Strategy<Value = SchemaVariant> {
    prop_oneof![Just(SchemaVariant::Base), Just(SchemaVariant::WithStatus),]
}

fn row_map_strategy() -> impl Strategy<Value = BTreeMap<i64, PropRow>> {
    btree_map(1_i64..48, prop_row_strategy(), 0..18)
}

fn prop_row_strategy() -> impl Strategy<Value = PropRow> {
    (
        option::of(text_value_strategy()),
        -500_i32..500_i32,
        option::of(vec(any::<u8>(), 0..6)),
        option::of(text_value_strategy()),
    )
        .prop_map(|(label, quantity, payload, status)| PropRow {
            label,
            quantity,
            payload,
            status,
        })
}

fn text_value_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just(String::from("O'Reilly")),
        "[a-z0-9 _-]{0,12}".prop_map(|value| value),
    ]
}

fn database_summary_strategy() -> impl Strategy<Value = DatabaseSummary> {
    btree_map(
        "[a-z][a-z0-9_]{0,7}",
        btree_map("[a-z][a-z0-9_]{0,7}", column_shape_strategy(), 1..5),
        0..5,
    )
    .prop_map(|tables| DatabaseSummary {
        path: String::from("generated.sqlite"),
        tables: tables
            .into_iter()
            .map(|(name, columns)| table_info(name, columns))
            .collect(),
        views: Vec::new(),
    })
}

fn column_shape_strategy() -> impl Strategy<Value = ColumnShape> {
    (
        prop_oneof![
            Just(String::from("INTEGER")),
            Just(String::from("REAL")),
            Just(String::from("TEXT")),
            Just(String::from("BLOB")),
        ],
        any::<bool>(),
        option::of(text_value_strategy()),
        any::<bool>(),
    )
        .prop_map(
            |(col_type, nullable, default_value, is_primary_key)| ColumnShape {
                col_type,
                nullable,
                default_value,
                is_primary_key,
            },
        )
}

fn table_info(name: String, columns: BTreeMap<String, ColumnShape>) -> TableInfo {
    let mut primary_key = Vec::new();
    let columns = columns
        .into_iter()
        .map(|(column_name, shape)| {
            if shape.is_primary_key {
                primary_key.push(column_name.clone());
            }

            ColumnInfo {
                name: column_name,
                col_type: shape.col_type,
                nullable: shape.nullable,
                default_value: shape.default_value,
                is_primary_key: shape.is_primary_key,
            }
        })
        .collect();

    TableInfo {
        name,
        columns,
        row_count: 0,
        primary_key,
        create_sql: None,
    }
}

impl SchemaVariant {
    fn create_table_sql(self) -> &'static str {
        match self {
            Self::Base => {
                "
                CREATE TABLE items (
                    id INTEGER PRIMARY KEY,
                    label TEXT,
                    quantity INTEGER NOT NULL,
                    payload BLOB
                );
                "
            }
            Self::WithStatus => {
                "
                CREATE TABLE items (
                    id INTEGER PRIMARY KEY,
                    label TEXT,
                    quantity INTEGER NOT NULL,
                    payload BLOB,
                    status TEXT DEFAULT 'active'
                );
                "
            }
        }
    }
}
