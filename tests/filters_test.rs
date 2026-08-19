// Integration tests for filter loading and validation.
//
// These run as a separate crate (not inside the binary), so they can ONLY
// access public items from `gmail_fetcher`. This is the key difference from
// inline `#[cfg(test)]` tests — integration tests enforce your public API.

use gmail_fetcher::filters::load_filters;
use std::io::Write;

use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Helper: writes content to a temp file and returns its path.
/// The file is automatically deleted when the TempFile is dropped.
struct TempFile {
    path: String,
}

impl TempFile {
    fn new(content: &str) -> Self {
        let dir = std::env::temp_dir().join("gmail_fetcher_integration_test");
        std::fs::create_dir_all(&dir).unwrap();
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("test_{}_{}.toml", std::process::id(), id));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        Self {
            path: path.to_str().unwrap().to_string(),
        }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn valid_config_loads() {
    let tf = TempFile::new(
        r#"
[[filters]]
name = "test-filter"
gmail_query = "from:test@example.com has:attachment"
"#,
    );
    let filters = load_filters(&tf.path).unwrap();
    assert_eq!(filters.len(), 1);
    assert_eq!(filters[0].name, "test-filter");
    assert_eq!(filters[0].gmail_query, "from:test@example.com has:attachment");
}

#[test]
fn multiple_filters_load() {
    let tf = TempFile::new(
        r#"
[[filters]]
name = "filter-a"
gmail_query = "from:a@example.com"

[[filters]]
name = "filter-b"
gmail_query = "from:b@example.com has:attachment"
"#,
    );
    let filters = load_filters(&tf.path).unwrap();
    assert_eq!(filters.len(), 2);
    assert_eq!(filters[0].name, "filter-a");
    assert_eq!(filters[1].name, "filter-b");
}

#[test]
fn empty_name_rejected() {
    let tf = TempFile::new(
        r#"
[[filters]]
name = ""
gmail_query = "from:test@example.com"
"#,
    );
    let err = load_filters(&tf.path).unwrap_err();
    assert!(
        err.to_string().contains("empty name"),
        "Expected 'empty name' error, got: {}",
        err
    );
}

#[test]
fn whitespace_only_name_rejected() {
    let tf = TempFile::new(
        r#"
[[filters]]
name = "   "
gmail_query = "from:test@example.com"
"#,
    );
    let err = load_filters(&tf.path).unwrap_err();
    assert!(err.to_string().contains("empty name"));
}

#[test]
fn empty_query_rejected() {
    let tf = TempFile::new(
        r#"
[[filters]]
name = "my-filter"
gmail_query = ""
"#,
    );
    let err = load_filters(&tf.path).unwrap_err();
    assert!(
        err.to_string().contains("empty gmail_query"),
        "Expected 'empty gmail_query' error, got: {}",
        err
    );
}

#[test]
fn whitespace_only_query_rejected() {
    let tf = TempFile::new(
        r#"
[[filters]]
name = "my-filter"
gmail_query = "   "
"#,
    );
    let err = load_filters(&tf.path).unwrap_err();
    assert!(err.to_string().contains("empty gmail_query"));
}

#[test]
fn no_filters_rejected() {
    let tf = TempFile::new("# empty config\n");
    let err = load_filters(&tf.path).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("No filters defined") || msg.contains("missing field"),
        "Expected 'no filters' or 'missing field' error, got: {}",
        msg
    );
}

#[test]
fn missing_file_rejected() {
    let err = load_filters("/nonexistent/path/filters.toml").unwrap_err();
    assert!(err.to_string().contains("Failed to read filters config"));
}

#[test]
fn invalid_toml_rejected() {
    let tf = TempFile::new("this is not valid toml [[[");
    let err = load_filters(&tf.path).unwrap_err();
    assert!(err.to_string().contains("Failed to parse filters config"));
}

#[test]
fn error_message_includes_filter_name() {
    // When an empty query is rejected, the error should name the filter
    let tf = TempFile::new(
        r#"
[[filters]]
name = "broken-filter"
gmail_query = ""
"#,
    );
    let err = load_filters(&tf.path).unwrap_err();
    assert!(
        err.to_string().contains("broken-filter"),
        "Error should include filter name, got: {}",
        err
    );
}
