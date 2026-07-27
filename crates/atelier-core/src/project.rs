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
    AssetId, AssetReference, AtelierDocument, ContentKind, DocumentError, PROJECT_FORMAT,
    PROJECT_SCHEMA_VERSION,
};

pub const PROJECT_EXTENSION: &str = "pcba";
const MANIFEST_PATH: &str = "manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
pub enum ProjectAssetCommand {
    MoveToFolder {
        asset_id: AssetId,
        folder_path: Option<String>,
    },
    ReplaceAllReferences {
        original_asset_id: AssetId,
        replacement_asset_id: AssetId,
    },
    Delete {
        asset_id: AssetId,
    },
    CleanupUnused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum ProjectAssetCommandOutcome {
    AssetMoved {
        asset_id: AssetId,
        folder_path: Option<String>,
    },
    ReferencesReplaced {
        original_asset_id: AssetId,
        replacement_asset_id: AssetId,
        instance_count: usize,
        treatment_count: usize,
    },
    AssetDeleted {
        asset_id: AssetId,
    },
    UnusedAssetsRemoved {
        asset_ids: Vec<AssetId>,
    },
}

impl ProjectAssetCommand {
    pub fn apply(
        self,
        bundle: &mut ProjectBundle,
    ) -> Result<ProjectAssetCommandOutcome, ProjectError> {
        match self {
            Self::MoveToFolder {
                asset_id,
                folder_path,
            } => {
                let folder_path = normalize_asset_folder_path(folder_path)?;
                let asset = bundle
                    .document
                    .assets
                    .iter_mut()
                    .find(|asset| asset.id == asset_id)
                    .ok_or(ProjectError::MissingAsset(asset_id))?;
                asset.folder_path = folder_path.clone();
                Ok(ProjectAssetCommandOutcome::AssetMoved {
                    asset_id,
                    folder_path,
                })
            }
            Self::ReplaceAllReferences {
                original_asset_id,
                replacement_asset_id,
            } => {
                if !bundle.assets.contains_key(&original_asset_id) {
                    return Err(ProjectError::MissingAsset(original_asset_id));
                }
                if !bundle.assets.contains_key(&replacement_asset_id) {
                    return Err(ProjectError::MissingAsset(replacement_asset_id));
                }
                let treatment_count = bundle
                    .document
                    .image_treatments
                    .iter()
                    .filter(|treatment| treatment.asset_id == original_asset_id)
                    .count();
                let instance_count =
                    bundle.replace_all_asset_references(original_asset_id, replacement_asset_id);
                Ok(ProjectAssetCommandOutcome::ReferencesReplaced {
                    original_asset_id,
                    replacement_asset_id,
                    instance_count,
                    treatment_count: if original_asset_id == replacement_asset_id {
                        0
                    } else {
                        treatment_count
                    },
                })
            }
            Self::Delete { asset_id } => {
                bundle.delete_asset(asset_id)?;
                Ok(ProjectAssetCommandOutcome::AssetDeleted { asset_id })
            }
            Self::CleanupUnused => {
                let asset_ids = bundle.cleanup_unused_assets();
                Ok(ProjectAssetCommandOutcome::UnusedAssetsRemoved { asset_ids })
            }
        }
    }
}

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
        let digest = sha256_hex(&bytes);
        if let Some(existing) = self
            .document
            .assets
            .iter()
            .find(|asset| asset.sha256 == digest)
        {
            return Ok(existing.id);
        }
        let extension = safe_extension(&original_filename, &media_type);
        let has_alpha = image::load_from_memory(&bytes)
            .map(|image| image.color().has_alpha())
            .unwrap_or(false);
        let id = AssetId::new();
        let reference = AssetReference {
            id,
            embedded_path: format!("assets/{id}.{extension}"),
            original_filename,
            media_type,
            sha256: digest,
            pixel_width,
            pixel_height,
            folder_path: None,
            tags: Vec::new(),
            has_alpha,
        };
        self.document.assets.push(reference);
        self.assets.insert(id, bytes);
        Ok(id)
    }

    pub fn asset_bytes(&self, asset_id: AssetId) -> Option<&[u8]> {
        self.assets.get(&asset_id).map(Vec::as_slice)
    }

    pub fn asset_usage_count(&self, asset_id: AssetId) -> usize {
        self.document
            .front
            .layers
            .iter()
            .chain(&self.document.back.layers)
            .filter(|layer| {
                matches!(&layer.kind, ContentKind::Image(image) if image.asset_id == asset_id)
            })
            .count()
    }

    pub fn search_assets(&self, query: &str) -> Vec<&AssetReference> {
        let query = query.trim().to_lowercase();
        self.document
            .assets
            .iter()
            .filter(|asset| {
                query.is_empty()
                    || asset.original_filename.to_lowercase().contains(&query)
                    || asset
                        .folder_path
                        .as_deref()
                        .is_some_and(|folder| folder.to_lowercase().contains(&query))
                    || asset
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query))
            })
            .collect()
    }

    pub fn replace_all_asset_references(
        &mut self,
        original_asset_id: AssetId,
        replacement_asset_id: AssetId,
    ) -> usize {
        if original_asset_id == replacement_asset_id
            || !self.assets.contains_key(&replacement_asset_id)
        {
            return 0;
        }
        let mut replaced = 0;
        for layer in self
            .document
            .front
            .layers
            .iter_mut()
            .chain(&mut self.document.back.layers)
        {
            if let ContentKind::Image(image) = &mut layer.kind
                && image.asset_id == original_asset_id
            {
                image.asset_id = replacement_asset_id;
                replaced += 1;
            }
        }
        for treatment in &mut self.document.image_treatments {
            if treatment.asset_id == original_asset_id {
                treatment.asset_id = replacement_asset_id;
            }
        }
        replaced
    }

    pub fn delete_asset(&mut self, asset_id: AssetId) -> Result<(), ProjectError> {
        let usage_count = self.asset_usage_count(asset_id);
        let treatment_count = self
            .document
            .image_treatments
            .iter()
            .filter(|treatment| treatment.asset_id == asset_id)
            .count();
        if usage_count + treatment_count > 0 {
            return Err(ProjectError::AssetInUse {
                asset_id,
                usage_count: usage_count + treatment_count,
            });
        }
        let index = self
            .document
            .assets
            .iter()
            .position(|asset| asset.id == asset_id)
            .ok_or(ProjectError::MissingAsset(asset_id))?;
        self.document.assets.remove(index);
        self.assets.remove(&asset_id);
        Ok(())
    }

    pub fn cleanup_unused_assets(&mut self) -> Vec<AssetId> {
        let unused = self
            .document
            .assets
            .iter()
            .filter(|asset| {
                self.asset_usage_count(asset.id) == 0
                    && !self
                        .document
                        .image_treatments
                        .iter()
                        .any(|treatment| treatment.asset_id == asset.id)
            })
            .map(|asset| asset.id)
            .collect::<Vec<_>>();
        for asset_id in &unused {
            self.document.assets.retain(|asset| asset.id != *asset_id);
            self.assets.remove(asset_id);
        }
        unused
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
            let mut deserializer = serde_json::Deserializer::from_reader(&mut entry);
            serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
                ProjectError::InvalidManifestField {
                    path: error.path().to_string(),
                    message: error.inner().to_string(),
                }
            })?
        };
        let document = manifest.into_document()?;
        let expected_archive_paths = std::iter::once(MANIFEST_PATH.to_owned())
            .chain(
                document
                    .assets
                    .iter()
                    .map(|asset| asset.embedded_path.clone()),
            )
            .collect::<HashSet<_>>();
        if let Some(missing_path) = expected_archive_paths
            .difference(&archive_paths)
            .next()
            .cloned()
        {
            return Err(ProjectError::MissingEntry(missing_path));
        }
        if let Some(unexpected_path) = archive_paths
            .difference(&expected_archive_paths)
            .next()
            .cloned()
        {
            return Err(ProjectError::UnexpectedArchiveEntry(unexpected_path));
        }
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
        validate_document_with_path(&self.document)?;
        let referenced_ids = self
            .document
            .assets
            .iter()
            .map(|asset| asset.id)
            .collect::<HashSet<_>>();
        for asset in &self.document.assets {
            let normalized_folder_path = normalize_asset_folder_path(asset.folder_path.clone())?;
            if normalized_folder_path != asset.folder_path {
                return Err(ProjectError::InvalidAssetFolderPath(
                    asset.folder_path.clone().unwrap_or_default(),
                ));
            }
            let bytes = self
                .assets
                .get(&asset.id)
                .ok_or(ProjectError::MissingAsset(asset.id))?;
            if sha256_hex(bytes) != asset.sha256 {
                return Err(ProjectError::AssetHashMismatch(asset.id));
            }
            if self.document.image_treatments.iter().any(|treatment| {
                treatment.asset_id == asset.id
                    && treatment.production_mode == crate::ImageProductionMode::ColorOriginal
            }) && !matches!(
                image::guess_format(bytes),
                Ok(image::ImageFormat::Png | image::ImageFormat::Jpeg)
            ) {
                return Err(ProjectError::UnsupportedColorOriginalAssetFormat {
                    asset_id: asset.id,
                });
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    fn into_document(self) -> Result<AtelierDocument, ProjectError> {
        if self.format != PROJECT_FORMAT {
            return Err(ProjectError::InvalidManifestFormat(self.format));
        }
        if self.schema_version != PROJECT_SCHEMA_VERSION {
            return Err(ProjectError::UnsupportedManifestSchema(self.schema_version));
        }
        if self.document.schema_version != self.schema_version {
            return Err(ProjectError::ManifestDocumentSchemaMismatch {
                manifest: self.schema_version,
                document: self.document.schema_version,
            });
        }
        validate_document_with_path(&self.document)?;
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
    #[error("asset folder path is invalid: {0}")]
    InvalidAssetFolderPath(String),
    #[error("project is missing embedded asset {0}")]
    MissingAsset(AssetId),
    #[error("project contains unreferenced embedded asset {0}")]
    UnreferencedAsset(AssetId),
    #[error("embedded asset hash does not match manifest for {0}")]
    AssetHashMismatch(AssetId),
    #[error("彩色原图素材 {asset_id} 的嵌入字节不是 PNG/JPEG；请重新导入 PNG 或 JPEG 原图")]
    UnsupportedColorOriginalAssetFormat { asset_id: AssetId },
    #[error("asset {asset_id} is still used by {usage_count} project references")]
    AssetInUse {
        asset_id: AssetId,
        usage_count: usize,
    },
    #[error("project archive is missing {0}")]
    MissingEntry(String),
    #[error("project archive contains unsafe path: {0}")]
    UnsafeArchivePath(String),
    #[error("project archive contains duplicate path: {0}")]
    DuplicateArchivePath(String),
    #[error("project archive contains unexpected entry: {0}")]
    UnexpectedArchiveEntry(String),
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
    #[error("project manifest field `{path}` is invalid: {message}")]
    InvalidManifestField { path: String, message: String },
    #[error("project field `{path}` is invalid: {message}")]
    InvalidDocumentField { path: String, message: String },
    #[error("project I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not atomically replace project: {0}")]
    Persist(#[from] tempfile::PersistError),
}

fn normalize_asset_folder_path(
    folder_path: Option<String>,
) -> Result<Option<String>, ProjectError> {
    let Some(folder_path) = folder_path else {
        return Ok(None);
    };
    let trimmed = folder_path.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > 255
        || trimmed.starts_with('/')
        || trimmed.contains('\\')
        || trimmed.chars().any(char::is_control)
    {
        return Err(ProjectError::InvalidAssetFolderPath(folder_path));
    }
    let segments = trimmed.split('/').map(str::trim).collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return Err(ProjectError::InvalidAssetFolderPath(folder_path));
    }
    Ok(Some(segments.join("/")))
}

fn validate_document_with_path(document: &AtelierDocument) -> Result<(), ProjectError> {
    document
        .validate()
        .map_err(|error| ProjectError::InvalidDocumentField {
            path: document_error_path(document, &error),
            message: error.to_string(),
        })
}

fn document_error_path(document: &AtelierDocument, error: &DocumentError) -> String {
    match error {
        DocumentError::InvalidFormat(_) => "document.format".to_owned(),
        DocumentError::UnsupportedSchema(_) => "document.schemaVersion".to_owned(),
        DocumentError::InvalidFaceAssignment => "document.front.side".to_owned(),
        DocumentError::InvalidBoardDimensions | DocumentError::InvalidCornerRadius { .. } => {
            "document.board".to_owned()
        }
        DocumentError::InvalidBoardThickness => "document.stackup.thicknessUm".to_owned(),
        DocumentError::DuplicateLayerId(layer_id)
        | DocumentError::LayerCycle(layer_id)
        | DocumentError::MissingMappedLayer(layer_id) => layer_path(document, *layer_id),
        DocumentError::EmptyLayerName { layer_id }
        | DocumentError::InvalidLayerSize { layer_id }
        | DocumentError::InvalidImageCrop { layer_id }
        | DocumentError::InvalidBoardFillClearance { layer_id, .. }
        | DocumentError::MissingImageAsset { layer_id, .. }
        | DocumentError::BoardFillMustMapToCopper { layer_id, .. }
        | DocumentError::DuplicateProductionMapping { layer_id, .. }
        | DocumentError::CrossFaceMapping { layer_id, .. } => layer_path(document, *layer_id),
        DocumentError::MultipleBoardFills { side, .. } => {
            format!("document.{}.layers", side)
        }
        DocumentError::MissingParent { layer_id, .. }
        | DocumentError::ParentIsNotGroup { layer_id, .. } => layer_path(document, *layer_id),
        DocumentError::DuplicateAssetId(_)
        | DocumentError::InvalidAssetMetadata { .. }
        | DocumentError::DuplicateAssetEmbeddedPath(_)
        | DocumentError::DuplicateAssetHash(_)
        | DocumentError::UnsafeAssetPath(_) => "document.assets".to_owned(),
        DocumentError::DuplicateTreatmentId(_)
        | DocumentError::MissingTreatmentAsset { .. }
        | DocumentError::InvalidTreatmentAlgorithmVersion { .. }
        | DocumentError::InvalidTreatmentCrop(_)
        | DocumentError::ColorOriginalRequiresMulticolorProfile { .. }
        | DocumentError::UnsupportedColorOriginalMedia { .. }
        | DocumentError::ColorOriginalRequiresSilkscreen { .. }
        | DocumentError::MissingMappedTreatment(_)
        | DocumentError::TreatmentAssetMismatch { .. } => "document.imageTreatments".to_owned(),
        DocumentError::InvalidManufacturerProfile(_) => "document.manufacturerProfile".to_owned(),
        DocumentError::DuplicateMappingId(_) => "document.mappings".to_owned(),
        DocumentError::InvalidMechanicalFeature | DocumentError::MechanicalFeatureOutsideBoard => {
            "document.mechanicalFeatures".to_owned()
        }
    }
}

fn layer_path(document: &AtelierDocument, layer_id: crate::LayerId) -> String {
    if let Some(index) = document
        .front
        .layers
        .iter()
        .position(|layer| layer.id == layer_id)
    {
        return format!("document.front.layers[{index}]");
    }
    if let Some(index) = document
        .back
        .layers
        .iter()
        .position(|layer| layer.id == layer_id)
    {
        return format!("document.back.layers[{index}]");
    }
    "document.mappings".to_owned()
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
