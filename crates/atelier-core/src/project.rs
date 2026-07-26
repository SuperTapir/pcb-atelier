use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use thiserror::Error;
use zip::write::SimpleFileOptions;

use crate::{
    AssetId, AssetReference, AtelierDocument, DocumentError, MIN_SUPPORTED_PROJECT_SCHEMA_VERSION,
    PROJECT_FORMAT, PROJECT_SCHEMA_VERSION,
};

pub const PROJECT_EXTENSION: &str = "pcba";
const MANIFEST_PATH: &str = "manifest.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectBundle {
    pub document: AtelierDocument,
    pub assets: BTreeMap<AssetId, Vec<u8>>,
}

impl ProjectBundle {
    pub fn new(document: AtelierDocument) -> Self {
        Self {
            document,
            assets: BTreeMap::new(),
        }
    }

    pub fn embed_asset(
        &mut self,
        original_filename: impl Into<String>,
        media_type: impl Into<String>,
        pixel_width: u32,
        pixel_height: u32,
        bytes: Vec<u8>,
    ) -> Result<AssetId, ProjectError> {
        if bytes.is_empty() {
            return Err(ProjectError::EmptyAsset);
        }
        if pixel_width == 0 || pixel_height == 0 {
            return Err(ProjectError::InvalidAssetDimensions);
        }
        let original_filename = original_filename.into();
        let media_type = media_type.into();
        let extension = safe_extension(&original_filename, &media_type);
        let id = AssetId::new();
        let reference = AssetReference {
            id,
            embedded_path: format!("assets/{id}.{extension}"),
            original_filename,
            media_type,
            sha256: sha256_hex(&bytes),
            pixel_width,
            pixel_height,
        };
        self.document.assets.push(reference);
        self.assets.insert(id, bytes);
        Ok(id)
    }

    pub fn asset_bytes(&self, asset_id: AssetId) -> Option<&[u8]> {
        self.assets.get(&asset_id).map(Vec::as_slice)
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        self.validate_assets()?;
        let parent = path.parent().ok_or(ProjectError::MissingParent)?;
        let temporary = NamedTempFile::new_in(parent)?;
        {
            let mut archive = zip::ZipWriter::new(temporary.reopen()?);
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644);
            archive.start_file(MANIFEST_PATH, options)?;
            serde_json::to_writer_pretty(&mut archive, &ProjectManifest::from(&self.document))?;
            for asset in &self.document.assets {
                archive.start_file(&asset.embedded_path, options)?;
                archive.write_all(
                    self.assets
                        .get(&asset.id)
                        .ok_or(ProjectError::MissingAsset(asset.id))?,
                )?;
            }
            archive.finish()?;
        }
        temporary.persist(path)?;
        Ok(())
    }

    pub fn open(path: &Path) -> Result<Self, ProjectError> {
        let mut archive = zip::ZipArchive::new(File::open(path)?)?;
        let mut archive_paths = HashSet::new();
        for index in 0..archive.len() {
            let entry = archive.by_index(index)?;
            validate_archive_path(entry.name())?;
            if !archive_paths.insert(entry.name().to_owned()) {
                return Err(ProjectError::DuplicateArchivePath(entry.name().to_owned()));
            }
        }

        let manifest: ProjectManifest = {
            let mut entry = archive
                .by_name(MANIFEST_PATH)
                .map_err(|_| ProjectError::MissingEntry(MANIFEST_PATH.to_owned()))?;
            serde_json::from_reader(&mut entry)?
        };
        let document = manifest.into_document()?;
        let mut assets = BTreeMap::new();
        for asset in &document.assets {
            let mut entry = archive
                .by_name(&asset.embedded_path)
                .map_err(|_| ProjectError::MissingEntry(asset.embedded_path.clone()))?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            if sha256_hex(&bytes) != asset.sha256 {
                return Err(ProjectError::AssetHashMismatch(asset.id));
            }
            assets.insert(asset.id, bytes);
        }
        let bundle = Self { document, assets };
        bundle.validate_assets()?;
        Ok(bundle)
    }

    fn validate_assets(&self) -> Result<(), ProjectError> {
        self.document.validate()?;
        let referenced_ids = self
            .document
            .assets
            .iter()
            .map(|asset| asset.id)
            .collect::<HashSet<_>>();
        for asset in &self.document.assets {
            let bytes = self
                .assets
                .get(&asset.id)
                .ok_or(ProjectError::MissingAsset(asset.id))?;
            if sha256_hex(bytes) != asset.sha256 {
                return Err(ProjectError::AssetHashMismatch(asset.id));
            }
        }
        if let Some(extra_id) = self
            .assets
            .keys()
            .find(|asset_id| !referenced_ids.contains(asset_id))
        {
            return Err(ProjectError::UnreferencedAsset(*extra_id));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectManifest {
    format: String,
    schema_version: u32,
    document: AtelierDocument,
}

impl From<&AtelierDocument> for ProjectManifest {
    fn from(document: &AtelierDocument) -> Self {
        let mut document = document.clone();
        document.schema_version = PROJECT_SCHEMA_VERSION;
        Self {
            format: PROJECT_FORMAT.to_owned(),
            schema_version: PROJECT_SCHEMA_VERSION,
            document,
        }
    }
}

impl ProjectManifest {
    fn into_document(mut self) -> Result<AtelierDocument, ProjectError> {
        if self.format != PROJECT_FORMAT {
            return Err(ProjectError::InvalidManifestFormat(self.format));
        }
        if !(MIN_SUPPORTED_PROJECT_SCHEMA_VERSION..=PROJECT_SCHEMA_VERSION)
            .contains(&self.schema_version)
        {
            return Err(ProjectError::UnsupportedManifestSchema(self.schema_version));
        }
        if self.document.schema_version != self.schema_version {
            return Err(ProjectError::ManifestDocumentSchemaMismatch {
                manifest: self.schema_version,
                document: self.document.schema_version,
            });
        }
        self.document.validate()?;
        self.document.schema_version = PROJECT_SCHEMA_VERSION;
        self.document.validate()?;
        Ok(self.document)
    }
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project path has no parent directory")]
    MissingParent,
    #[error("embedded asset is empty")]
    EmptyAsset,
    #[error("embedded asset dimensions must be positive")]
    InvalidAssetDimensions,
    #[error("project is missing embedded asset {0}")]
    MissingAsset(AssetId),
    #[error("project contains unreferenced embedded asset {0}")]
    UnreferencedAsset(AssetId),
    #[error("embedded asset hash does not match manifest for {0}")]
    AssetHashMismatch(AssetId),
    #[error("project archive is missing {0}")]
    MissingEntry(String),
    #[error("project archive contains unsafe path: {0}")]
    UnsafeArchivePath(String),
    #[error("project archive contains duplicate path: {0}")]
    DuplicateArchivePath(String),
    #[error("project manifest format is invalid: {0}")]
    InvalidManifestFormat(String),
    #[error("project manifest schema version is unsupported: {0}")]
    UnsupportedManifestSchema(u32),
    #[error("project manifest schema {manifest} does not match document schema {document}")]
    ManifestDocumentSchemaMismatch { manifest: u32, document: u32 },
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error("project archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("project JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("project I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not atomically replace project: {0}")]
    Persist(#[from] tempfile::PersistError),
}

fn safe_extension(filename: &str, media_type: &str) -> String {
    let extension = Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 10
                && extension
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        });
    extension.unwrap_or_else(|| match media_type {
        "image/png" => "png".to_owned(),
        "image/jpeg" => "jpg".to_owned(),
        "image/webp" => "webp".to_owned(),
        "image/svg+xml" => "svg".to_owned(),
        _ => "bin".to_owned(),
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_archive_path(path: &str) -> Result<PathBuf, ProjectError> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ProjectError::UnsafeArchivePath(path.to_owned()));
    }
    Ok(candidate)
}
