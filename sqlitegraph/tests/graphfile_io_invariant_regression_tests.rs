//! GraphFile I/O Invariant Regression Tests
//!
//! This test ensures the file-size invariant prevents "failed to fill whole buffer" errors
//! by validating file size before any read_exact operation.

use sqlitegraph::backend::native::{GraphFile, NativeBackendError};
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_read_bytes_direct_file_size_invariant() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary file with known size
    let mut temp_file = NamedTempFile::new()?;
    let test_data = b"Hello, World!"; // 13 bytes
    temp_file.write_all(test_data)?;
    temp_file.flush()?;

    // Open with GraphFile
    let mut graph_file = GraphFile::open(temp_file.path())?;

    // Test 1: Valid read should succeed
    let mut buffer = vec![0u8; 10];
    let result = graph_file.read_bytes_direct(0, &mut buffer);
    assert!(result.is_ok(), "Valid read should succeed: {:?}", result);

    // Test 2: Reading beyond file size should fail with detailed error
    let mut large_buffer = vec![0u8; 100];
    let result = graph_file.read_bytes_direct(0, &mut large_buffer);
    assert!(result.is_err(), "Reading beyond file size should fail");

    match result.unwrap_err() {
        NativeBackendError::CorruptNodeRecord { node_id, reason } => {
            assert_eq!(node_id, -1, "Should be system-level error with node_id=-1");
            assert!(
                reason.contains("File too small for read"),
                "Error should mention file too small"
            );
            assert!(
                reason.contains("need 100 bytes"),
                "Error should show needed bytes"
            );
            assert!(
                reason.contains("but file is only"),
                "Error should show actual file size"
            );
        }
        other => panic!(
            "Expected CorruptNodeRecord with file size details, got: {:?}",
            other
        ),
    }

    // Test 3: Reading at offset beyond file should fail
    let mut buffer = vec![0u8; 5];
    let result = graph_file.read_bytes_direct(20, &mut buffer);
    assert!(result.is_err(), "Reading at offset beyond file should fail");

    Ok(())
}

#[test]
fn test_read_header_file_size_invariant() -> Result<(), Box<dyn std::error::Error>> {
    // Create a file smaller than header size
    let temp_file = NamedTempFile::new()?;

    // Open with GraphFile
    let mut graph_file = GraphFile::open(temp_file.path())?;

    // Reading header should fail with detailed error
    let result = graph_file.read_header();
    assert!(result.is_err(), "Reading header from tiny file should fail");

    match result.unwrap_err() {
        NativeBackendError::CorruptNodeRecord { node_id, reason } => {
            assert_eq!(node_id, -1, "Should be system-level error");
            assert!(
                reason.contains("File too small for read"),
                "Error should mention file too small"
            );
        }
        other => panic!(
            "Expected CorruptNodeRecord with file size details, got: {:?}",
            other
        ),
    }

    Ok(())
}

#[test]
fn test_read_edge_at_offset_file_size_invariant() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary file with small size
    let temp_file = NamedTempFile::new()?;

    // Open with GraphFile
    let mut graph_file = GraphFile::create(temp_file.path())?;

    // Test reading edge beyond file size - should return None gracefully
    let result = graph_file.read_edge_at_offset(1000000); // 1MB offset
    assert!(
        result.is_none(),
        "Reading edge beyond file should return None"
    );

    Ok(())
}

#[test]
fn test_detailed_error_message_format() -> Result<(), Box<dyn std::error::Error>> {
    // Test that error messages contain all necessary details
    let mut temp_file = NamedTempFile::new()?;

    // Write exactly 50 bytes
    temp_file.as_file().write_all(&vec![0u8; 50])?;
    temp_file.flush()?;

    let mut graph_file = GraphFile::open(temp_file.path())?;

    // Try to read 100 bytes at offset 25 (would need 125 bytes total)
    let mut buffer = vec![0u8; 100];
    let result = graph_file.read_bytes_direct(25, &mut buffer);
    assert!(result.is_err());

    let error_msg = format!("{}", result.unwrap_err());
    assert!(error_msg.contains("File too small for read"));
    assert!(error_msg.contains("need 100 bytes"));
    assert!(error_msg.contains("starting at offset 25"));
    assert!(error_msg.contains("125 total"));
    assert!(error_msg.contains("file is only 50 bytes"));
    assert!(error_msg.contains("Missing 75 bytes"));

    Ok(())
}

#[test]
fn test_invariant_prevents_failed_to_fill_whole_buffer() -> Result<(), Box<dyn std::error::Error>> {
    // This test specifically ensures we never get "failed to fill whole buffer"
    // from std::io::read_exact, but instead get our detailed error

    let temp_file = NamedTempFile::new()?;
    let mut graph_file = GraphFile::open(temp_file.path())?;

    // Attempt a read that would previously cause "failed to fill whole buffer"
    let mut huge_buffer = vec![0u8; 1_000_000]; // 1MB buffer
    let result = graph_file.read_bytes_direct(0, &mut huge_buffer);

    // Should fail with our detailed error, not "failed to fill whole buffer"
    assert!(result.is_err());
    let error_str = result.unwrap_err().to_string();

    // Verify we get the expected detailed error, not the generic std::io error
    assert!(error_str.contains("File too small for read"));
    assert!(!error_str.contains("failed to fill whole buffer"));

    Ok(())
}
