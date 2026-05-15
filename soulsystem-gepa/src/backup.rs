use anyhow::Result;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Create a timestamped backup of the given file in the same directory.
/// Returns the path to the backup file.
pub fn backup_file(path: &Path) -> Result<String> {
    let code = fs_err::read_to_string(path)?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let backup_name = format!(
        "{}.backup.{}.rs",
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("skill"),
        stamp
    );

    let backup_path = path.with_file_name(&backup_name);
    fs_err::write(&backup_path, &code)?;
    tracing::info!("Backup created at {:?}", backup_path);

    Ok(backup_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_backup_file() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("test.rs");
        std::fs::write(&p, "pub fn x() {}").unwrap();
        let name = backup_file(&p).unwrap();
        assert!(name.starts_with("test.backup."));
        assert!(name.ends_with(".rs"));
    }
}
