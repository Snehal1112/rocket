//! Parser for runner iteration data files (CSV and JSON). Pure text rules, no I/O.

use rocket_shared::error::{DomainError, DomainResult};
use std::collections::{BTreeMap, HashSet};

/// The largest data file the runner reads.
pub const MAX_DATA_FILE_BYTES: usize = 10 * 1024 * 1024;
/// The most rows one data file may hold.
pub const MAX_DATA_ROWS: usize = 1000;

/// One data row: column name to text value.
pub type IterationRow = BTreeMap<String, String>;

/// A parsed data file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IterationData {
    /// Column names in file order (JSON: first seen).
    pub columns: Vec<String>,
    pub rows: Vec<IterationRow>,
}

/// The file formats the runner reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Csv,
    Json,
}

fn invalid(message: impl Into<String>) -> DomainError {
    DomainError::InvalidInput(message.into())
}

/// Picks the format from the file extension, case-insensitive.
pub fn format_for_path(path: &str) -> DomainResult<DataFormat> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = match name.rsplit_once('.') {
        Some((_, ext)) => ext.to_ascii_lowercase(),
        None => String::new(),
    };
    match ext.as_str() {
        "csv" => Ok(DataFormat::Csv),
        "json" => Ok(DataFormat::Json),
        _ => Err(invalid("The data file must end in .csv or .json.")),
    }
}

/// Parses data text. The text must hold at least one row and at most `MAX_DATA_ROWS`.
pub fn parse_iteration_data(format: DataFormat, text: &str) -> DomainResult<IterationData> {
    if text.len() > MAX_DATA_FILE_BYTES {
        return Err(invalid("The data file is too large."));
    }
    let data = match format {
        DataFormat::Csv => parse_csv(text)?,
        DataFormat::Json => parse_json(text)?,
    };
    if data.rows.is_empty() {
        return Err(invalid("The data file has no rows."));
    }
    Ok(data)
}

fn too_many_rows() -> DomainError {
    invalid(format!("The data file has more than {MAX_DATA_ROWS} rows."))
}

fn parse_csv(text: &str) -> DomainResult<IterationData> {
    let text = text.trim_start_matches('\u{feff}');
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| invalid(format!("The CSV header could not be read: {e}")))?
        .clone();
    let columns: Vec<String> = headers.iter().map(str::to_string).collect();
    let mut seen = HashSet::new();
    for column in &columns {
        if !seen.insert(column.as_str()) {
            return Err(invalid(format!(
                "The CSV has a duplicate header: {column}."
            )));
        }
    }
    let mut rows = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let number = index + 1;
        let record = record.map_err(|e| invalid(format!("CSV row {number} is not valid: {e}")))?;
        if record.len() > columns.len() {
            return Err(invalid(format!(
                "CSV row {number} has more cells than the header."
            )));
        }
        if rows.len() >= MAX_DATA_ROWS {
            return Err(too_many_rows());
        }
        let row = columns
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), record.get(i).unwrap_or("").to_string()))
            .collect();
        rows.push(row);
    }
    Ok(IterationData { columns, rows })
}

fn value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn parse_json(text: &str) -> DomainResult<IterationData> {
    let text = text.trim_start_matches('\u{feff}');
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| invalid(format!("The JSON could not be read: {e}")))?;
    let items = match value {
        serde_json::Value::Array(items) => items,
        _ => return Err(invalid("The JSON must be an array of objects.")),
    };
    if items.len() > MAX_DATA_ROWS {
        return Err(too_many_rows());
    }
    let mut columns: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let number = index + 1;
        let object = item
            .as_object()
            .ok_or_else(|| invalid(format!("JSON row {number} is not an object.")))?;
        let mut row = IterationRow::new();
        for (key, value) in object {
            if !columns.contains(key) {
                columns.push(key.clone());
            }
            row.insert(key.clone(), value_text(value));
        }
        rows.push(row);
    }
    Ok(IterationData { columns, rows })
}

/// Checks a saved default data file path: non-empty, relative, `/` separators, no `..`.
pub fn validate_data_file_path(path: &str) -> DomainResult<()> {
    if path.is_empty() {
        return Err(invalid("The data file path is empty."));
    }
    if path.starts_with('/') || path.contains('\\') || path.contains(':') || path.contains('\0') {
        return Err(invalid(
            "The data file path must be relative and use / separators.",
        ));
    }
    if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid("The data file path has an empty, . or .. part."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pairs: &[(&str, &str)]) -> IterationRow {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn err_text<T>(r: DomainResult<T>) -> String {
        match r {
            Ok(_) => panic!("expected an error"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn csv_reads_header_and_rows() {
        let d = parse_iteration_data(DataFormat::Csv, "a,b\n1,2\n3,4\n").expect("parse");
        assert_eq!(d.columns, vec!["a", "b"]);
        assert_eq!(
            d.rows,
            vec![
                row(&[("a", "1"), ("b", "2")]),
                row(&[("a", "3"), ("b", "4")])
            ]
        );
    }

    #[test]
    fn csv_strips_a_leading_bom() {
        let d = parse_iteration_data(DataFormat::Csv, "\u{feff}a,b\n1,2\n").expect("parse");
        assert_eq!(d.columns, vec!["a", "b"]);
    }

    #[test]
    fn csv_keeps_quoted_newline_and_comma_in_one_cell() {
        let d = parse_iteration_data(DataFormat::Csv, "a,b\n\"x,\ny\",2\n").expect("parse");
        assert_eq!(d.rows[0]["a"], "x,\ny");
        assert_eq!(d.rows[0]["b"], "2");
    }

    #[test]
    fn csv_pads_a_short_row_with_empty_strings() {
        let d = parse_iteration_data(DataFormat::Csv, "a,b,c\n1\n").expect("parse");
        assert_eq!(d.rows[0], row(&[("a", "1"), ("b", ""), ("c", "")]));
    }

    #[test]
    fn csv_rejects_a_long_row_and_names_the_row() {
        let msg = err_text(parse_iteration_data(DataFormat::Csv, "a,b\n1,2\n1,2,3\n"));
        assert!(msg.contains("row 2"), "{msg}");
    }

    #[test]
    fn csv_skips_blank_lines() {
        let d = parse_iteration_data(DataFormat::Csv, "a\n1\n\n2\n").expect("parse");
        assert_eq!(d.rows.len(), 2);
    }

    #[test]
    fn csv_rejects_a_duplicate_header() {
        let msg = err_text(parse_iteration_data(DataFormat::Csv, "a,a\n1,2\n"));
        assert!(msg.contains("duplicate"), "{msg}");
    }

    #[test]
    fn json_stringifies_scalars_null_and_nested_values() {
        let text = r#"[{"n":3,"b":true,"z":null,"o":{"a":1},"s":"hi"}]"#;
        let d = parse_iteration_data(DataFormat::Json, text).expect("parse");
        let r = &d.rows[0];
        assert_eq!(r["n"], "3");
        assert_eq!(r["b"], "true");
        assert_eq!(r["z"], "");
        assert_eq!(r["o"], r#"{"a":1}"#);
        assert_eq!(r["s"], "hi");
    }

    #[test]
    fn json_rejects_a_non_array_top_level() {
        assert!(parse_iteration_data(DataFormat::Json, r#"{"a":1}"#).is_err());
    }

    #[test]
    fn json_row_without_a_key_has_no_entry_for_it() {
        let d =
            parse_iteration_data(DataFormat::Json, r#"[{"a":1,"b":2},{"a":3}]"#).expect("parse");
        assert_eq!(d.columns, vec!["a", "b"]);
        assert!(!d.rows[1].contains_key("b"));
    }

    #[test]
    fn empty_data_is_an_error() {
        assert!(parse_iteration_data(DataFormat::Csv, "").is_err());
        assert!(parse_iteration_data(DataFormat::Csv, "a,b\n").is_err());
        assert!(parse_iteration_data(DataFormat::Json, "[]").is_err());
    }

    #[test]
    fn over_1000_rows_is_an_error() {
        let mut csv = String::from("a\n");
        for i in 0..1001 {
            csv.push_str(&format!("{i}\n"));
        }
        assert!(parse_iteration_data(DataFormat::Csv, &csv).is_err());
        let ok: String = csv.lines().take(1001).collect::<Vec<_>>().join("\n");
        assert_eq!(
            parse_iteration_data(DataFormat::Csv, &ok)
                .expect("parse")
                .rows
                .len(),
            1000
        );
        let json = format!("[{}]", vec!["{\"a\":1}"; 1001].join(","));
        assert!(parse_iteration_data(DataFormat::Json, &json).is_err());
    }

    #[test]
    fn format_for_path_is_case_insensitive_and_rejects_other_extensions() {
        assert_eq!(format_for_path("d/Data.CSV").expect("csv"), DataFormat::Csv);
        assert_eq!(format_for_path("x.json").expect("json"), DataFormat::Json);
        assert!(format_for_path("x.txt").is_err());
        assert!(format_for_path("noext").is_err());
    }

    #[test]
    fn validate_data_file_path_rejects_empty_absolute_dotdot_and_backslash_paths() {
        for p in [
            "",
            "/abs/x.csv",
            "../x.csv",
            "a/../x.csv",
            "a\\x.csv",
            "C:/x.csv",
        ] {
            assert!(validate_data_file_path(p).is_err(), "{p}");
        }
    }

    #[test]
    fn validate_data_file_path_accepts_a_nested_relative_path() {
        assert!(validate_data_file_path("data/users.csv").is_ok());
        assert!(validate_data_file_path("users.json").is_ok());
    }
}
