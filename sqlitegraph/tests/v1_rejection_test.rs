use sqlitegraph::{GraphConfig, open_graph};
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

#[test]
fn test_v1_files_rejected_hard_gate() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_v1_rejection.db");

    // Create a V1-style file (magic bytes but no V2 flags)
    let mut file = File::create(&db_path)?;

    // Write V1 header (magic + version = 1, no V2 flags)
    let v1_magic = [b'S', b'Q', b'L', b'T', b'G', b'F', 0, 0];
    let v1_version = 1u32;
    let v1_flags = 0u32; // No V2 flags

    file.write_all(&v1_magic)?;
    file.write_all(&v1_version.to_be_bytes())?;
    file.write_all(&v1_flags.to_be_bytes())?;

    // Pad rest of header with zeros
    let header_padding = vec![0u8; 1024 - 16];
    file.write_all(&header_padding)?;

    drop(file);

    // Attempt to open with V2-only backend - should fail
    let result = open_graph(&db_path, &GraphConfig::native());

    assert!(
        result.is_err(),
        "V1 file should be rejected by V2-only backend"
    );

    match result.unwrap_err() {
        sqlitegraph::SqliteGraphError::NativeBackendError(native_err) => match native_err {
            sqlitegraph::backend::native::NativeBackendError::UnsupportedVersion {
                version,
                reason,
            } => {
                assert_eq!(version, 1);
                assert!(reason.contains("V2-only"));
                println!("SUCCESS: V1 file correctly rejected: {}", reason);
            }
            other => panic!("Expected UnsupportedVersion error, got: {:?}", other),
        },
        other => panic!("Expected NativeBackendError, got: {:?}", other),
    }

    Ok(())
}
