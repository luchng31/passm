//! Persistent device identity (UUIDv4 stored in `<app_data>/device_id`).

use crate::error::SyncError;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use uuid::Uuid;

const DEVICE_ID_FILE: &str = "device_id";

/// Reads the persisted device id, if present.
pub fn load(dir: &Path) -> Result<Option<String>, SyncError> {
    match fs::read_to_string(dir.join(DEVICE_ID_FILE)) {
        Ok(content) => {
            let id = content.trim();
            if Uuid::parse_str(id).is_err() {
                return Err(SyncError::InvalidDeviceId);
            }
            Ok(Some(id.to_string()))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(SyncError::Io(e)),
    }
}

/// Returns the persisted device id, generating and persisting a fresh UUIDv4
/// on first run (creating `dir` if needed).
pub fn load_or_create(dir: &Path) -> Result<String, SyncError> {
    if let Some(existing) = load(dir)? {
        return Ok(existing);
    }
    fs::create_dir_all(dir).map_err(SyncError::Io)?;
    let id = Uuid::new_v4().to_string();
    fs::write(dir.join(DEVICE_ID_FILE), &id).map_err(SyncError::Io)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_then_reload_same_value() {
        let dir = tempdir().unwrap();
        let first = load_or_create(dir.path()).unwrap();
        let second = load_or_create(dir.path()).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn missing_file_generates_new_uuid() {
        let dir = tempdir().unwrap();
        assert_eq!(load(dir.path()).unwrap(), None);
        let id = load_or_create(dir.path()).unwrap();
        assert!(Uuid::parse_str(&id).is_ok());
        assert_eq!(load(dir.path()).unwrap(), Some(id));
    }

    #[test]
    fn directory_missing_creates_it() {
        let base = tempdir().unwrap();
        let nested = base.path().join("nested").join("deeper");
        let id = load_or_create(&nested).unwrap();
        assert!(nested.join(DEVICE_ID_FILE).exists());
        assert_eq!(load(&nested).unwrap(), Some(id));
    }

    #[test]
    fn invalid_existing_id_is_rejected() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(DEVICE_ID_FILE), "not-a-uuid").unwrap();
        assert!(matches!(
            load(dir.path()).unwrap_err(),
            SyncError::InvalidDeviceId
        ));
        assert!(matches!(
            load_or_create(dir.path()).unwrap_err(),
            SyncError::InvalidDeviceId
        ));
    }

    #[test]
    fn load_missing_dir_returns_none() {
        let base = tempdir().unwrap();
        let missing = base.path().join("does-not-exist");
        assert_eq!(load(&missing).unwrap(), None);
    }

    #[test]
    fn generated_id_is_uuidv4() {
        let dir = tempdir().unwrap();
        let id = load_or_create(dir.path()).unwrap();
        let uuid = Uuid::parse_str(&id).unwrap();
        assert_eq!(uuid.get_version_num(), 4);
    }
}
