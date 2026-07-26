//! Traceable, versioned two-stage EasyEDA handoff.
//!
//! The public archive and native project are separately written beside their
//! final destinations, validated, and atomically replaced. Only after both
//! validated artifacts exist do we atomically write the manifest/report.

use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    EasyedaNativeError, EasyedaNativeValidation, EasyedaPublicError, PublicArchiveValidation,
    ResolvedFabricationBoard, atomic_write_validated, convert_easyeda_archive_to_native,
    export_public_archive, validate_public_archive,
};

pub const EASYEDA_HANDOFF_EXPORT_FORMAT_VERSION: &str = "atelier-easyeda-handoff-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaPrimitiveStatistics {
    pub fill_count: usize,
    pub hole_count: usize,
    pub filled_layer_ids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaHandoffExportReport {
    pub export_format_version: String,
    pub export_version: String,
    pub manifest_path: PathBuf,
    pub public_archive_path: PathBuf,
    pub native_project_path: PathBuf,
    /// SHA-256 of the fabrication plan, grid and rasterizer inputs.
    pub fabrication_input_sha256: String,
    /// SHA-256 of the six resolved production masks, before format adaptation.
    pub fabrication_output_sha256: String,
    pub public_archive_sha256: String,
    pub native_project_sha256: String,
    pub primitives: EasyedaPrimitiveStatistics,
    pub public_validation: PublicArchiveValidation,
    pub native_validation: EasyedaNativeValidation,
}

#[derive(Debug, Error)]
pub enum EasyedaHandoffError {
    #[error("EasyEDA handoff output directory could not be prepared: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Public(#[from] EasyedaPublicError),
    #[error(transparent)]
    Native(#[from] EasyedaNativeError),
}

/// Write a new, immutable downstream export version for one resolved board.
///
/// The source board is not modified. Repeating the export allocates the next
/// versioned basename, so an EDA user's local edits stay isolated from later
/// Atelier exports.
pub fn export_easyeda_handoff(
    destination_directory: &Path,
    title: &str,
    board: &ResolvedFabricationBoard,
) -> Result<EasyedaHandoffExportReport, EasyedaHandoffError> {
    fs::create_dir_all(destination_directory)?;
    let (export_version, public_archive_path, native_project_path, manifest_path) =
        allocate_versioned_paths(destination_directory, title, &board.build.output_sha256)?;

    let public = export_public_archive(&public_archive_path, title, board)?;
    let public_validation = validate_public_archive(&public_archive_path)?;
    let native = convert_easyeda_archive_to_native(&public_archive_path, &native_project_path)?;
    let report = EasyedaHandoffExportReport {
        export_format_version: EASYEDA_HANDOFF_EXPORT_FORMAT_VERSION.to_owned(),
        export_version,
        manifest_path,
        public_archive_path,
        native_project_path,
        fabrication_input_sha256: board.build.input_sha256.clone(),
        fabrication_output_sha256: board.build.output_sha256.clone(),
        public_archive_sha256: sha256_file(&public.archive_path)?,
        native_project_sha256: sha256_file(&native.project_path)?,
        primitives: EasyedaPrimitiveStatistics {
            fill_count: public.fill_count,
            hole_count: public.hole_count,
            filled_layer_ids: public_validation.filled_layer_ids.clone(),
        },
        public_validation,
        native_validation: native.validation,
    };
    write_manifest_atomically(&report)?;
    Ok(report)
}

fn allocate_versioned_paths(
    destination_directory: &Path,
    title: &str,
    fabrication_output_sha256: &str,
) -> Result<(String, PathBuf, PathBuf, PathBuf), io::Error> {
    let slug = title_slug(title);
    let fingerprint = fabrication_output_sha256
        .get(..12)
        .unwrap_or(fabrication_output_sha256);
    for sequence in 1_u32.. {
        let export_version = format!("{slug}-{fingerprint}-v{sequence:04}");
        let public = destination_directory.join(format!("{export_version}.epro2"));
        let native = destination_directory.join(format!("{export_version}.eprj2"));
        let manifest = destination_directory.join(format!("{export_version}.manifest.json"));
        if !public.exists() && !native.exists() && !manifest.exists() {
            return Ok((export_version, public, native, manifest));
        }
    }
    unreachable!("u32 export version space is exhaustive")
}

fn write_manifest_atomically(report: &EasyedaHandoffExportReport) -> Result<(), io::Error> {
    atomic_write_validated(
        &report.manifest_path,
        |file| serde_json::to_writer_pretty(file, report).map_err(io::Error::other),
        |path| {
            let actual =
                serde_json::from_reader::<_, EasyedaHandoffExportReport>(File::open(path)?)
                    .map_err(io::Error::other)?;
            if actual == *report {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "handoff manifest does not round-trip",
                ))
            }
        },
    )
    .map_err(|error| match error {
        crate::AtomicWriteError::Io(error)
        | crate::AtomicWriteError::Write(error)
        | crate::AtomicWriteError::Validation(error) => error,
        crate::AtomicWriteError::Persist(error) => error.error,
    })
}

fn title_slug(title: &str) -> String {
    let slug = title
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "pcb-atelier".to_owned()
    } else {
        slug.to_owned()
    }
}

fn sha256_file(path: &Path) -> Result<String, io::Error> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
