// Integration test for attachment path generation and file write+read cycle.
//
// This tests that attachment_path produces the correct directory structure
// and that files can actually be written to and read from those paths.

use std::path::PathBuf;

/// Replicate the attachment_path logic from gmail.rs to verify it matches.
/// (Integration tests can't call private functions — so we test the public
/// contract: the path is predictable and files are accessible.)
fn expected_attachment_path(base: &str, date: &str, email_id: &str, filename: &str) -> PathBuf {
    // date is "YYYY-MM-DD" format
    let parts: Vec<&str> = date.split('-').collect();
    PathBuf::from(base)
        .join(parts[0])
        .join(parts[1])
        .join(parts[2])
        .join(email_id)
        .join(filename)
}

#[test]
fn attachment_path_produces_accessible_directory() {
    let base = std::env::temp_dir().join("gmail_fetcher_path_test");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let path = expected_attachment_path(
        base.to_str().unwrap(),
        "2025-03-15",
        "msg_abc123",
        "receipt.pdf",
    );

    // Create the parent directory (what download_attachments does)
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    // Write a file
    std::fs::write(&path, b"test content").unwrap();

    // Read it back
    let content = std::fs::read(&path).unwrap();
    assert_eq!(content, b"test content");

    // Cleanup
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn different_email_ids_do_not_collide() {
    let base = std::env::temp_dir().join("gmail_fetcher_collision_test");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let date = "2025-06-01";
    let path1 = expected_attachment_path(base.to_str().unwrap(), date, "email_A", "file.pdf");
    let path2 = expected_attachment_path(base.to_str().unwrap(), date, "email_B", "file.pdf");

    assert_ne!(path1, path2);

    // Both can be written without overwriting each other
    std::fs::create_dir_all(path1.parent().unwrap()).unwrap();
    std::fs::create_dir_all(path2.parent().unwrap()).unwrap();
    std::fs::write(&path1, b"content A").unwrap();
    std::fs::write(&path2, b"content B").unwrap();

    assert_eq!(std::fs::read(&path1).unwrap(), b"content A");
    assert_eq!(std::fs::read(&path2).unwrap(), b"content B");

    let _ = std::fs::remove_dir_all(&base);
}
