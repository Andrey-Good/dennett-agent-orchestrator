use dennett_local_ipc::LocalEndpoint;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const METADATA_VERSION: u32 = 1;
const METADATA_FILE: &str = "installation.json";
const MAX_METADATA_BYTES: u64 = 4 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct InstallationMetadata {
    pub version: u32,
    pub installation_id: String,
    pub device_id: String,
    pub authority_epoch: u64,
}

impl InstallationMetadata {
    fn generate() -> Self {
        Self {
            version: METADATA_VERSION,
            installation_id: uuid::Uuid::now_v7().to_string(),
            device_id: uuid::Uuid::now_v7().to_string(),
            authority_epoch: 1,
        }
    }

    fn validate(&self) -> Result<(), InstallationError> {
        if self.version != METADATA_VERSION
            || self.authority_epoch == 0
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.device_id.chars().any(char::is_control)
            || LocalEndpoint::for_installation(&self.installation_id).is_err()
        {
            return Err(InstallationError::InvalidMetadata);
        }
        Ok(())
    }
}

pub(super) async fn load_or_create(
    data_dir: PathBuf,
) -> Result<InstallationMetadata, InstallationError> {
    tokio::task::spawn_blocking(move || load_or_create_blocking(&data_dir))
        .await
        .map_err(|_| InstallationError::StorageUnavailable)?
}

fn load_or_create_blocking(data_dir: &Path) -> Result<InstallationMetadata, InstallationError> {
    fs::create_dir_all(data_dir).map_err(|_| InstallationError::StorageUnavailable)?;
    let path = data_dir.join(METADATA_FILE);
    match read(&path) {
        Ok(metadata) => return Ok(metadata),
        Err(InstallationError::NotFound) => {}
        Err(error) => return Err(error),
    }

    let metadata = InstallationMetadata::generate();
    let encoded =
        serde_json::to_vec_pretty(&metadata).map_err(|_| InstallationError::InvalidMetadata)?;
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            file.write_all(&encoded)
                .and_then(|_| file.sync_all())
                .map_err(|_| InstallationError::StorageUnavailable)?;
            Ok(metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read(&path),
        Err(_) => Err(InstallationError::StorageUnavailable),
    }
}

fn read(path: &Path) -> Result<InstallationMetadata, InstallationError> {
    let file_metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            InstallationError::NotFound
        } else {
            InstallationError::StorageUnavailable
        }
    })?;
    if file_metadata.file_type().is_symlink()
        || !file_metadata.is_file()
        || file_metadata.len() == 0
        || file_metadata.len() > MAX_METADATA_BYTES
    {
        return Err(InstallationError::InvalidMetadata);
    }
    let mut encoded = Vec::with_capacity(file_metadata.len() as usize);
    OpenOptions::new()
        .read(true)
        .open(path)
        .and_then(|file| file.take(MAX_METADATA_BYTES + 1).read_to_end(&mut encoded))
        .map_err(|_| InstallationError::StorageUnavailable)?;
    let metadata: InstallationMetadata =
        serde_json::from_slice(&encoded).map_err(|_| InstallationError::InvalidMetadata)?;
    metadata.validate()?;
    Ok(metadata)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InstallationError {
    NotFound,
    InvalidMetadata,
    StorageUnavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn installation_identity_is_created_once_and_reused() {
        let directory = tempfile::tempdir().expect("tempdir");
        let first = load_or_create(directory.path().to_owned())
            .await
            .expect("create metadata");
        let second = load_or_create(directory.path().to_owned())
            .await
            .expect("read metadata");
        assert_eq!(first, second);
        assert_eq!(first.authority_epoch, 1);
    }

    #[tokio::test]
    async fn malformed_identity_is_not_silently_replaced() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(directory.path().join(METADATA_FILE), b"{}").expect("write invalid metadata");
        assert_eq!(
            load_or_create(directory.path().to_owned()).await,
            Err(InstallationError::InvalidMetadata)
        );
    }
}
