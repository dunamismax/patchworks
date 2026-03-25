//! Semantic diff awareness: detects table renames, column renames, and compatible type shifts.

use std::collections::BTreeSet;

use crate::db::types::{
    ColumnInfo, DatabaseSummary, SchemaDiff, SemanticChange, TableInfo, TableSchemaDiff,
};

/// Minimum column overlap ratio to consider a table rename candidate.
const TABLE_RENAME_MIN_OVERLAP: f64 = 0.7;
/// Minimum column overlap ratio to consider a column rename candidate.
const COLUMN_RENAME_MIN_CONFIDENCE: u8 = 60;

/// Detects semantic changes (renames, compatible type shifts) from a schema diff.
pub fn detect_semantic_changes(
    left: &DatabaseSummary,
    right: &DatabaseSummary,
    schema_diff: &SchemaDiff,
) -> Vec<SemanticChange> {
    let mut changes = Vec::new();

    detect_table_renames(
        &schema_diff.removed_tables,
        &schema_diff.added_tables,
        &mut changes,
    );

    detect_column_renames(&schema_diff.modified_tables, &mut changes);

    detect_compatible_type_shifts(left, right, &schema_diff.modified_tables, &mut changes);

    changes
}

/// Looks for tables that were "removed" on the left and "added" on the right with
/// similar column structures, suggesting a rename.
fn detect_table_renames(
    removed: &[TableInfo],
    added: &[TableInfo],
    changes: &mut Vec<SemanticChange>,
) {
    if removed.is_empty() || added.is_empty() {
        return;
    }

    let mut claimed_added: BTreeSet<usize> = BTreeSet::new();

    for removed_table in removed {
        let mut best_match: Option<(usize, u8)> = None;

        for (index, added_table) in added.iter().enumerate() {
            if claimed_added.contains(&index) {
                continue;
            }

            let confidence = column_similarity(removed_table, added_table);
            if confidence >= (TABLE_RENAME_MIN_OVERLAP * 100.0) as u8
                && best_match.map_or(true, |(_, best)| confidence > best)
            {
                best_match = Some((index, confidence));
            }
        }

        if let Some((index, confidence)) = best_match {
            claimed_added.insert(index);
            changes.push(SemanticChange::TableRename {
                left_name: removed_table.name.clone(),
                right_name: added[index].name.clone(),
                confidence,
            });
        }
    }
}

/// Looks for columns removed and added within the same table that have the same type
/// and properties, suggesting a rename.
fn detect_column_renames(modified_tables: &[TableSchemaDiff], changes: &mut Vec<SemanticChange>) {
    for table_diff in modified_tables {
        if table_diff.removed_columns.is_empty() || table_diff.added_columns.is_empty() {
            continue;
        }

        let mut claimed_added: BTreeSet<usize> = BTreeSet::new();

        for removed_col in &table_diff.removed_columns {
            let mut best_match: Option<(usize, u8)> = None;

            for (index, added_col) in table_diff.added_columns.iter().enumerate() {
                if claimed_added.contains(&index) {
                    continue;
                }

                let confidence = column_rename_confidence(removed_col, added_col);
                if confidence >= COLUMN_RENAME_MIN_CONFIDENCE
                    && best_match.map_or(true, |(_, best)| confidence > best)
                {
                    best_match = Some((index, confidence));
                }
            }

            if let Some((index, confidence)) = best_match {
                claimed_added.insert(index);
                changes.push(SemanticChange::ColumnRename {
                    table_name: table_diff.table_name.clone(),
                    left_column: removed_col.name.clone(),
                    right_column: table_diff.added_columns[index].name.clone(),
                    confidence,
                });
            }
        }
    }
}

/// Detects compatible type changes within modified columns.
fn detect_compatible_type_shifts(
    _left: &DatabaseSummary,
    _right: &DatabaseSummary,
    modified_tables: &[TableSchemaDiff],
    changes: &mut Vec<SemanticChange>,
) {
    for table_diff in modified_tables {
        for (left_col, right_col) in &table_diff.modified_columns {
            if left_col.name == right_col.name
                && !left_col.col_type.eq_ignore_ascii_case(&right_col.col_type)
                && is_compatible_type_shift(&left_col.col_type, &right_col.col_type)
            {
                changes.push(SemanticChange::CompatibleTypeShift {
                    table_name: table_diff.table_name.clone(),
                    column_name: left_col.name.clone(),
                    left_type: left_col.col_type.clone(),
                    right_type: right_col.col_type.clone(),
                });
            }
        }
    }
}

/// Returns a similarity score (0-100) between two tables based on column overlap.
fn column_similarity(left: &TableInfo, right: &TableInfo) -> u8 {
    if left.columns.is_empty() && right.columns.is_empty() {
        return 100;
    }
    if left.columns.is_empty() || right.columns.is_empty() {
        return 0;
    }

    let left_names: BTreeSet<&str> = left.columns.iter().map(|c| c.name.as_str()).collect();
    let right_names: BTreeSet<&str> = right.columns.iter().map(|c| c.name.as_str()).collect();

    let intersection = left_names.intersection(&right_names).count();
    let union = left_names.union(&right_names).count();

    if union == 0 {
        return 0;
    }

    let jaccard = intersection as f64 / union as f64;

    // Bonus: check if matching columns also have matching types
    let mut type_matches = 0;
    for name in left_names.intersection(&right_names) {
        let left_col = left.columns.iter().find(|c| c.name == *name).unwrap();
        let right_col = right.columns.iter().find(|c| c.name == *name).unwrap();
        if left_col.col_type.eq_ignore_ascii_case(&right_col.col_type) {
            type_matches += 1;
        }
    }
    let type_ratio = if intersection > 0 {
        type_matches as f64 / intersection as f64
    } else {
        0.0
    };

    // Weighted: 60% column name overlap, 40% type match for shared columns
    let score = (jaccard * 0.6 + type_ratio * jaccard * 0.4) * 100.0;
    score.min(100.0) as u8
}

/// Returns a confidence score (0-100) that a removed column was renamed to an added column.
fn column_rename_confidence(removed: &ColumnInfo, added: &ColumnInfo) -> u8 {
    let mut score: u8 = 0;

    // Type match is the strongest signal
    if removed.col_type.eq_ignore_ascii_case(&added.col_type) {
        score += 40;
    } else if is_compatible_type_shift(&removed.col_type, &added.col_type) {
        score += 20;
    }

    // Nullable match
    if removed.nullable == added.nullable {
        score += 20;
    }

    // Primary key match
    if removed.is_primary_key == added.is_primary_key {
        score += 15;
    }

    // Default value match
    if removed.default_value == added.default_value {
        score += 15;
    }

    // Name similarity bonus (Levenshtein-ish)
    let name_sim = name_similarity(&removed.name, &added.name);
    score += (name_sim * 10.0) as u8;

    score.min(100)
}

/// Very simple name similarity: ratio of common characters to max length.
fn name_similarity(a: &str, b: &str) -> f64 {
    let a_lower = a.to_ascii_lowercase();
    let b_lower = b.to_ascii_lowercase();

    if a_lower == b_lower {
        return 1.0;
    }

    let max_len = a_lower.len().max(b_lower.len());
    if max_len == 0 {
        return 1.0;
    }

    // Count longest common subsequence length
    let a_bytes = a_lower.as_bytes();
    let b_bytes = b_lower.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    let mut prev = vec![0u16; n + 1];
    let mut curr = vec![0u16; n + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a_bytes[i - 1] == b_bytes[j - 1] {
                curr[j] = prev[j - 1] + 1;
            } else {
                curr[j] = prev[j].max(curr[j - 1]);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.iter_mut().for_each(|v| *v = 0);
    }

    prev[n] as f64 / max_len as f64
}

/// Returns true if the type change is likely compatible (e.g. INT -> INTEGER, TEXT -> VARCHAR).
pub fn is_compatible_type_shift(left: &str, right: &str) -> bool {
    let left_upper = left.to_ascii_uppercase();
    let right_upper = right.to_ascii_uppercase();

    if left_upper == right_upper {
        return true;
    }

    // SQLite type affinity families
    let left_affinity = sqlite_type_affinity(&left_upper);
    let right_affinity = sqlite_type_affinity(&right_upper);

    // Same affinity is generally compatible
    if left_affinity == right_affinity {
        return true;
    }

    // INTEGER to REAL is widening (compatible)
    if left_affinity == TypeAffinity::Integer && right_affinity == TypeAffinity::Real {
        return true;
    }

    // TEXT to BLOB or vice versa in SQLite is often cosmetic
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypeAffinity {
    Integer,
    Real,
    Text,
    Blob,
    Numeric,
}

/// Determines SQLite type affinity from a declared type string per SQLite's rules.
fn sqlite_type_affinity(declared: &str) -> TypeAffinity {
    let upper = declared.to_ascii_uppercase();

    // Rule 1: contains "INT"
    if upper.contains("INT") {
        return TypeAffinity::Integer;
    }

    // Rule 2: contains "CHAR", "CLOB", or "TEXT"
    if upper.contains("CHAR") || upper.contains("CLOB") || upper.contains("TEXT") {
        return TypeAffinity::Text;
    }

    // Rule 3: contains "BLOB" or is empty
    if upper.contains("BLOB") || upper.is_empty() {
        return TypeAffinity::Blob;
    }

    // Rule 4: contains "REAL", "FLOA", or "DOUB"
    if upper.contains("REAL") || upper.contains("FLOA") || upper.contains("DOUB") {
        return TypeAffinity::Real;
    }

    // Rule 5: otherwise NUMERIC
    TypeAffinity::Numeric
}

/// Determines whether two `SqlValue`s are semantically equivalent despite potential
/// cosmetic differences (e.g. integer 1 vs real 1.0, text "1" vs integer 1 when
/// the column affinity is numeric).
pub fn values_semantically_equal(
    left: &crate::db::types::SqlValue,
    right: &crate::db::types::SqlValue,
    column_type: &str,
) -> bool {
    use crate::db::types::SqlValue;

    if left == right {
        return true;
    }

    match (left, right) {
        // NULL is only equal to NULL
        (SqlValue::Null, _) | (_, SqlValue::Null) => false,

        // Integer and Real comparison: 1 == 1.0
        (SqlValue::Integer(l), SqlValue::Real(r)) => (*l as f64) == *r,
        (SqlValue::Real(l), SqlValue::Integer(r)) => *l == (*r as f64),

        // For numeric-affinity columns, text "1" and integer 1 may be cosmetically different
        (SqlValue::Text(t), SqlValue::Integer(i)) | (SqlValue::Integer(i), SqlValue::Text(t))
            if sqlite_type_affinity(column_type) == TypeAffinity::Integer
                || sqlite_type_affinity(column_type) == TypeAffinity::Numeric =>
        {
            t.parse::<i64>().ok() == Some(*i)
        }

        (SqlValue::Text(t), SqlValue::Real(r)) | (SqlValue::Real(r), SqlValue::Text(t))
            if sqlite_type_affinity(column_type) == TypeAffinity::Real
                || sqlite_type_affinity(column_type) == TypeAffinity::Numeric =>
        {
            t.parse::<f64>().ok() == Some(*r)
        }

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::types::{
        ColumnInfo, DatabaseSummary, SchemaDiff, SemanticChange, SqlValue, TableInfo,
        TableSchemaDiff,
    };

    fn col(name: &str, col_type: &str, pk: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.to_owned(),
            col_type: col_type.to_owned(),
            nullable: !pk,
            default_value: None,
            is_primary_key: pk,
        }
    }

    fn table(name: &str, columns: Vec<ColumnInfo>) -> TableInfo {
        let primary_key = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();
        TableInfo {
            name: name.to_owned(),
            columns,
            row_count: 0,
            primary_key,
            create_sql: None,
        }
    }

    fn empty_summary() -> DatabaseSummary {
        DatabaseSummary {
            path: String::new(),
            tables: Vec::new(),
            views: Vec::new(),
            indexes: Vec::new(),
            triggers: Vec::new(),
        }
    }

    #[test]
    fn detects_table_rename_by_column_overlap() {
        let left = empty_summary();
        let right = empty_summary();
        let schema_diff = SchemaDiff {
            added_tables: vec![table(
                "customers",
                vec![
                    col("id", "INTEGER", true),
                    col("name", "TEXT", false),
                    col("email", "TEXT", false),
                ],
            )],
            removed_tables: vec![table(
                "users",
                vec![
                    col("id", "INTEGER", true),
                    col("name", "TEXT", false),
                    col("email", "TEXT", false),
                ],
            )],
            modified_tables: Vec::new(),
            unchanged_tables: Vec::new(),
            added_indexes: Vec::new(),
            removed_indexes: Vec::new(),
            modified_indexes: Vec::new(),
            added_triggers: Vec::new(),
            removed_triggers: Vec::new(),
            modified_triggers: Vec::new(),
        };

        let changes = detect_semantic_changes(&left, &right, &schema_diff);
        assert_eq!(changes.len(), 1);
        match &changes[0] {
            SemanticChange::TableRename {
                left_name,
                right_name,
                confidence,
            } => {
                assert_eq!(left_name, "users");
                assert_eq!(right_name, "customers");
                assert!(*confidence >= 70);
            }
            other => panic!("expected TableRename, got {other:?}"),
        }
    }

    #[test]
    fn does_not_detect_rename_for_dissimilar_tables() {
        let left = empty_summary();
        let right = empty_summary();
        let schema_diff = SchemaDiff {
            added_tables: vec![table(
                "orders",
                vec![
                    col("order_id", "INTEGER", true),
                    col("total", "REAL", false),
                ],
            )],
            removed_tables: vec![table(
                "users",
                vec![
                    col("id", "INTEGER", true),
                    col("name", "TEXT", false),
                    col("email", "TEXT", false),
                ],
            )],
            modified_tables: Vec::new(),
            unchanged_tables: Vec::new(),
            added_indexes: Vec::new(),
            removed_indexes: Vec::new(),
            modified_indexes: Vec::new(),
            added_triggers: Vec::new(),
            removed_triggers: Vec::new(),
            modified_triggers: Vec::new(),
        };

        let changes = detect_semantic_changes(&left, &right, &schema_diff);
        assert!(
            changes
                .iter()
                .all(|c| !matches!(c, SemanticChange::TableRename { .. })),
            "should not detect rename for dissimilar tables"
        );
    }

    #[test]
    fn detects_column_rename() {
        let left = empty_summary();
        let right = empty_summary();
        let schema_diff = SchemaDiff {
            added_tables: Vec::new(),
            removed_tables: Vec::new(),
            modified_tables: vec![TableSchemaDiff {
                table_name: "users".to_owned(),
                added_columns: vec![col("full_name", "TEXT", false)],
                removed_columns: vec![col("name", "TEXT", false)],
                modified_columns: Vec::new(),
            }],
            unchanged_tables: Vec::new(),
            added_indexes: Vec::new(),
            removed_indexes: Vec::new(),
            modified_indexes: Vec::new(),
            added_triggers: Vec::new(),
            removed_triggers: Vec::new(),
            modified_triggers: Vec::new(),
        };

        let changes = detect_semantic_changes(&left, &right, &schema_diff);
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, SemanticChange::ColumnRename { table_name, left_column, right_column, .. }
                    if table_name == "users" && left_column == "name" && right_column == "full_name"
                )),
            "expected column rename detection, got {changes:?}"
        );
    }

    #[test]
    fn detects_compatible_type_shift() {
        let left = empty_summary();
        let right = empty_summary();
        let schema_diff = SchemaDiff {
            added_tables: Vec::new(),
            removed_tables: Vec::new(),
            modified_tables: vec![TableSchemaDiff {
                table_name: "items".to_owned(),
                added_columns: Vec::new(),
                removed_columns: Vec::new(),
                modified_columns: vec![(
                    col("count", "INT", false),
                    col("count", "INTEGER", false),
                )],
            }],
            unchanged_tables: Vec::new(),
            added_indexes: Vec::new(),
            removed_indexes: Vec::new(),
            modified_indexes: Vec::new(),
            added_triggers: Vec::new(),
            removed_triggers: Vec::new(),
            modified_triggers: Vec::new(),
        };

        let changes = detect_semantic_changes(&left, &right, &schema_diff);
        assert!(
            changes.iter().any(
                |c| matches!(c, SemanticChange::CompatibleTypeShift { table_name, column_name, .. }
                    if table_name == "items" && column_name == "count")
            ),
            "expected compatible type shift, got {changes:?}"
        );
    }

    #[test]
    fn type_affinity_classification() {
        assert_eq!(sqlite_type_affinity("INTEGER"), TypeAffinity::Integer);
        assert_eq!(sqlite_type_affinity("INT"), TypeAffinity::Integer);
        assert_eq!(sqlite_type_affinity("BIGINT"), TypeAffinity::Integer);
        assert_eq!(sqlite_type_affinity("TINYINT"), TypeAffinity::Integer);
        assert_eq!(sqlite_type_affinity("TEXT"), TypeAffinity::Text);
        assert_eq!(sqlite_type_affinity("VARCHAR(255)"), TypeAffinity::Text);
        assert_eq!(sqlite_type_affinity("CLOB"), TypeAffinity::Text);
        assert_eq!(sqlite_type_affinity("BLOB"), TypeAffinity::Blob);
        assert_eq!(sqlite_type_affinity("REAL"), TypeAffinity::Real);
        assert_eq!(sqlite_type_affinity("FLOAT"), TypeAffinity::Real);
        assert_eq!(sqlite_type_affinity("DOUBLE"), TypeAffinity::Real);
        assert_eq!(sqlite_type_affinity("NUMERIC"), TypeAffinity::Numeric);
        assert_eq!(sqlite_type_affinity("BOOLEAN"), TypeAffinity::Numeric);
    }

    #[test]
    fn compatible_type_shifts() {
        // Same affinity
        assert!(is_compatible_type_shift("INT", "INTEGER"));
        assert!(is_compatible_type_shift("INT", "BIGINT"));
        assert!(is_compatible_type_shift("TEXT", "VARCHAR(255)"));
        assert!(is_compatible_type_shift("FLOAT", "DOUBLE"));

        // Widening
        assert!(is_compatible_type_shift("INTEGER", "REAL"));

        // Incompatible
        assert!(!is_compatible_type_shift("TEXT", "INTEGER"));
        assert!(!is_compatible_type_shift("BLOB", "TEXT"));
        assert!(!is_compatible_type_shift("REAL", "INTEGER"));
    }

    #[test]
    fn semantic_value_equality() {
        // Integer == Real with same value
        assert!(values_semantically_equal(
            &SqlValue::Integer(1),
            &SqlValue::Real(1.0),
            "NUMERIC"
        ));

        // Text "42" == Integer 42 for numeric column
        assert!(values_semantically_equal(
            &SqlValue::Text("42".to_owned()),
            &SqlValue::Integer(42),
            "INTEGER"
        ));

        // Text "42" != Integer 42 for text column (affinity matters)
        assert!(!values_semantically_equal(
            &SqlValue::Text("42".to_owned()),
            &SqlValue::Integer(42),
            "TEXT"
        ));

        // NULL is never semantically equal to non-null
        assert!(!values_semantically_equal(
            &SqlValue::Null,
            &SqlValue::Integer(0),
            "INTEGER"
        ));
    }

    #[test]
    fn name_similarity_scores() {
        // "name" vs "full_name": LCS = "name" (4), max_len = 9 -> ~0.44
        let sim = name_similarity("name", "full_name");
        assert!(sim > 0.3 && sim < 0.6, "name vs full_name: {sim}");

        // "email" vs "email_address": LCS = "email" (5), max_len = 13 -> ~0.38
        let sim = name_similarity("email", "email_address");
        assert!(sim > 0.3 && sim < 0.5, "email vs email_address: {sim}");

        // Completely different
        let sim = name_similarity("abc", "xyz");
        assert!(sim < 0.01, "abc vs xyz: {sim}");

        // Identical
        let sim = name_similarity("id", "id");
        assert!((sim - 1.0).abs() < 0.01, "id vs id: {sim}");
    }
}
