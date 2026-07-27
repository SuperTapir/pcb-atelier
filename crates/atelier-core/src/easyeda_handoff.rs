//! Traceable, versioned two-stage EasyEDA handoff.
//!
//! The public archive and native project are separately written beside their
//! final destinations, validated, and atomically replaced. Only after both
//! validated artifacts exist do we atomically write the manifest/report.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    AssetId, AtelierDocument, CharacterProcess, CopperWeight, CropRect, EasyedaNativeError,
    EasyedaNativeValidation, EasyedaPublicError, ImageProductionMode, LayerId,
    ManufacturerProfileSnapshot, MappingId, ProductionTarget, ProjectBundle,
    PublicArchiveValidation, ResolvedFabricationBoard, SamplingPurpose, SolderMaskColor,
    SubstrateMaterial, SurfaceFinish, TransformUm, TreatmentId, atomic_write_validated,
    convert_easyeda_archive_to_native, export_public_archive, validate_public_archive,
};

pub const EASYEDA_HANDOFF_EXPORT_FORMAT_VERSION: &str = "atelier-easyeda-handoff-v3";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaPrimitiveStatistics {
    pub fill_count: usize,
    pub hole_count: usize,
    pub filled_layer_ids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaImageGraphicTrace {
    pub mapping_id: MappingId,
    pub source_instance_id: LayerId,
    pub target: ProductionTarget,
    pub treatment_id: Option<TreatmentId>,
    pub algorithm_version: Option<String>,
    pub recipe_fingerprint: Option<String>,
    pub asset_id: AssetId,
    pub asset_sha256: String,
    pub asset_media_type: String,
    pub production_mode: ImageProductionMode,
    pub mask_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorSilkscreenHandoffStatus {
    RequiresEasyedaProColorSilkscreenExport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaColorSilkscreenResource {
    pub status: ColorSilkscreenHandoffStatus,
    pub resource_path: PathBuf,
    pub mapping_id: MappingId,
    pub source_instance_id: LayerId,
    pub treatment_id: TreatmentId,
    pub target: ProductionTarget,
    pub asset_id: AssetId,
    pub original_filename: String,
    pub media_type: String,
    pub asset_sha256: String,
    pub transform: TransformUm,
    pub crop: Option<CropRect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaManufacturingSummary {
    pub validated: bool,
    pub manufacturer_id: String,
    pub profile_version: String,
    pub substrate: SubstrateMaterial,
    pub layer_count: u8,
    pub thickness_um: u32,
    pub outer_copper: CopperWeight,
    pub solder_mask: SolderMaskColor,
    pub character_process: CharacterProcess,
    pub surface_finish: SurfaceFinish,
}

impl From<&ManufacturerProfileSnapshot> for EasyedaManufacturingSummary {
    fn from(profile: &ManufacturerProfileSnapshot) -> Self {
        Self {
            validated: true,
            manufacturer_id: profile.manufacturer_id.clone(),
            profile_version: profile.profile_version.clone(),
            substrate: profile.substrate,
            layer_count: profile.layer_count,
            thickness_um: profile.thickness_um,
            outer_copper: profile.outer_copper,
            solder_mask: profile.solder_mask,
            character_process: profile.character_process,
            surface_finish: profile.surface_finish,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EasyedaOrderSupportStatus {
    DirectOrderSupported,
    RequiresManualAdjustment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaOrderSupport {
    pub status: EasyedaOrderSupportStatus,
    pub direct_order_supported: bool,
    pub issues: Vec<String>,
    pub downgrade_actions: Vec<String>,
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
    pub production_source: SamplingPurpose,
    pub image_graphics: Vec<EasyedaImageGraphicTrace>,
    pub color_silkscreen_resources: Vec<EasyedaColorSilkscreenResource>,
    pub manufacturing: EasyedaManufacturingSummary,
    pub order_support: EasyedaOrderSupport,
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
    #[error(
        "EasyEDA handoff requires formalProduction masks at {required_pitch_um} um, got {actual_purpose:?} at {actual_pitch_um} um"
    )]
    NonFormalProduction {
        required_pitch_um: u32,
        actual_pitch_um: u32,
        actual_purpose: Option<SamplingPurpose>,
    },
    #[error("EasyEDA handoff source or manufacturing validation failed: {0}")]
    InvalidSource(String),
}

/// Write a new, immutable downstream export version for one resolved board.
///
/// The source board is not modified. Repeating the export allocates the next
/// versioned basename, so an EDA user's local edits stay isolated from later
/// Atelier exports.
pub fn export_easyeda_handoff(
    destination_directory: &Path,
    bundle: &ProjectBundle,
    board: &ResolvedFabricationBoard,
) -> Result<EasyedaHandoffExportReport, EasyedaHandoffError> {
    let document = &bundle.document;
    validate_export_source(document, board)?;
    let image_graphics = image_graphic_traces(document, board)?;
    let manufacturing = EasyedaManufacturingSummary::from(&document.manufacturer_profile);
    fs::create_dir_all(destination_directory)?;
    let (export_version, public_archive_path, native_project_path, manifest_path) =
        allocate_versioned_paths(
            destination_directory,
            &document.title,
            &board.build.output_sha256,
        )?;
    let color_silkscreen_resources =
        write_color_silkscreen_resources(destination_directory, &export_version, bundle)?;
    let order_support = order_support(&document.manufacturer_profile, &color_silkscreen_resources);

    let public = export_public_archive(&public_archive_path, &document.title, board)?;
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
        production_source: SamplingPurpose::FormalProduction,
        image_graphics,
        color_silkscreen_resources,
        manufacturing,
        order_support,
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

fn validate_export_source(
    document: &AtelierDocument,
    board: &ResolvedFabricationBoard,
) -> Result<(), EasyedaHandoffError> {
    document
        .validate()
        .map_err(|error| EasyedaHandoffError::InvalidSource(error.to_string()))?;
    let required_pitch_um = SamplingPurpose::FormalProduction.default_pixel_pitch_um();
    if board.build.sampling_purpose != Some(SamplingPurpose::FormalProduction)
        || board.build.pixel_pitch_um != required_pitch_um
    {
        return Err(EasyedaHandoffError::NonFormalProduction {
            required_pitch_um,
            actual_pitch_um: board.build.pixel_pitch_um,
            actual_purpose: board.build.sampling_purpose,
        });
    }
    if board.outline != document.board || board.stackup != document.stackup {
        return Err(EasyedaHandoffError::InvalidSource(
            "resolved board does not match the validated document".to_owned(),
        ));
    }
    let profile = &document.manufacturer_profile;
    if profile.substrate != document.stackup.substrate
        || profile.thickness_um != document.stackup.thickness_um
        || profile.solder_mask != document.stackup.solder_mask_color
        || profile.surface_finish != document.stackup.surface_finish
    {
        return Err(EasyedaHandoffError::InvalidSource(
            "manufacturing profile does not match the resolved board stackup".to_owned(),
        ));
    }
    Ok(())
}

fn image_graphic_traces(
    document: &AtelierDocument,
    board: &ResolvedFabricationBoard,
) -> Result<Vec<EasyedaImageGraphicTrace>, EasyedaHandoffError> {
    let source_layers = document
        .front
        .layers
        .iter()
        .chain(&document.back.layers)
        .map(|layer| (layer.id, layer))
        .collect::<std::collections::HashMap<_, _>>();
    let mappings = document
        .mappings
        .iter()
        .map(|mapping| (mapping.id, mapping))
        .collect::<std::collections::HashMap<_, _>>();
    let assets = document
        .assets
        .iter()
        .map(|asset| (asset.id, asset))
        .collect::<std::collections::HashMap<_, _>>();
    let treatments = document
        .image_treatments
        .iter()
        .map(|treatment| (treatment.id, treatment))
        .collect::<std::collections::HashMap<_, _>>();
    let mut traces = Vec::new();

    for layer in &board.layers {
        for operation in &layer.operations {
            let Some(source) = source_layers.get(&operation.source_layer_id) else {
                return Err(EasyedaHandoffError::InvalidSource(format!(
                    "formal production operation refers to missing instance {}",
                    operation.source_layer_id
                )));
            };
            let crate::ContentKind::Image(image) = &source.kind else {
                continue;
            };
            let Some(mapping) = mappings.get(&operation.mapping_id) else {
                return Err(EasyedaHandoffError::InvalidSource(format!(
                    "formal production operation refers to missing mapping {}",
                    operation.mapping_id
                )));
            };
            let Some(asset) = assets.get(&image.asset_id) else {
                return Err(EasyedaHandoffError::InvalidSource(format!(
                    "image instance {} refers to missing asset {}",
                    source.id, image.asset_id
                )));
            };
            let treatment = mapping
                .treatment_id
                .map(|id| {
                    treatments.get(&id).copied().ok_or_else(|| {
                        EasyedaHandoffError::InvalidSource(format!(
                            "mapping {} refers to missing treatment {id}",
                            mapping.id
                        ))
                    })
                })
                .transpose()?;
            traces.push(EasyedaImageGraphicTrace {
                mapping_id: mapping.id,
                source_instance_id: source.id,
                target: layer.target,
                treatment_id: treatment.map(|value| value.id),
                algorithm_version: treatment.map(|value| value.recipe.algorithm_version.clone()),
                recipe_fingerprint: treatment.map(|value| value.recipe.fingerprint()),
                asset_id: asset.id,
                asset_sha256: asset.sha256.clone(),
                asset_media_type: asset.media_type.clone(),
                production_mode: treatment
                    .map(|value| value.production_mode)
                    .unwrap_or(ImageProductionMode::MonochromeMask),
                mask_sha256: operation.mask_sha256.clone(),
            });
        }
    }
    Ok(traces)
}

fn write_color_silkscreen_resources(
    destination_directory: &Path,
    export_version: &str,
    bundle: &ProjectBundle,
) -> Result<Vec<EasyedaColorSilkscreenResource>, EasyedaHandoffError> {
    let document = &bundle.document;
    let mut resources = Vec::new();
    for mapping in &document.mappings {
        let Some(treatment_id) = mapping.treatment_id else {
            continue;
        };
        let Some(treatment) = document
            .image_treatments
            .iter()
            .find(|treatment| treatment.id == treatment_id)
        else {
            continue;
        };
        if treatment.production_mode != ImageProductionMode::ColorOriginal {
            continue;
        }
        let source = document
            .front
            .layers
            .iter()
            .chain(&document.back.layers)
            .find(|layer| layer.id == mapping.source_layer_id)
            .ok_or_else(|| {
                EasyedaHandoffError::InvalidSource(format!(
                    "彩色丝印映射 {} 缺少源图片实例",
                    mapping.id
                ))
            })?;
        let crate::ContentKind::Image(image) = &source.kind else {
            return Err(EasyedaHandoffError::InvalidSource(format!(
                "彩色丝印映射 {} 的源实例不是图片",
                mapping.id
            )));
        };
        let asset = document
            .assets
            .iter()
            .find(|asset| asset.id == image.asset_id)
            .ok_or_else(|| {
                EasyedaHandoffError::InvalidSource(format!(
                    "彩色丝印实例 {} 缺少原始素材 {}",
                    source.id, image.asset_id
                ))
            })?;
        let bytes = bundle.asset_bytes(asset.id).ok_or_else(|| {
            EasyedaHandoffError::InvalidSource(format!(
                "彩色丝印素材 {} 缺少嵌入原图字节；请重新导入 PNG/JPEG",
                asset.id
            ))
        })?;
        let extension = match image::guess_format(bytes) {
            Ok(image::ImageFormat::Png) if asset.media_type == "image/png" => "png",
            Ok(image::ImageFormat::Jpeg) if asset.media_type == "image/jpeg" => "jpg",
            _ => {
                return Err(EasyedaHandoffError::InvalidSource(format!(
                    "彩色丝印素材 {} 的声明类型与原图字节不匹配；请重新导入 PNG/JPEG",
                    asset.id
                )));
            }
        };
        let actual_sha256 = format!("{:x}", Sha256::digest(bytes));
        if actual_sha256 != asset.sha256 {
            return Err(EasyedaHandoffError::InvalidSource(format!(
                "彩色丝印素材 {} 的原图哈希不匹配；请重新打开或重新导入工程素材",
                asset.id
            )));
        }
        let side = match mapping.target.side {
            crate::CardSide::Front => "front",
            crate::CardSide::Back => "back",
        };
        let resource_path = destination_directory.join(format!(
            "{export_version}.color-silkscreen-{side}-{}.{}",
            mapping.id, extension
        ));
        atomic_write_validated(
            &resource_path,
            |file| file.write_all(bytes),
            |path| {
                let written = fs::read(path)?;
                if format!("{:x}", Sha256::digest(&written)) == asset.sha256 {
                    Ok(())
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "color silkscreen resource hash mismatch",
                    ))
                }
            },
        )
        .map_err(|error| match error {
            crate::AtomicWriteError::Io(error)
            | crate::AtomicWriteError::Write(error)
            | crate::AtomicWriteError::Validation(error) => EasyedaHandoffError::Io(error),
            crate::AtomicWriteError::Persist(error) => EasyedaHandoffError::Io(error.error),
        })?;
        resources.push(EasyedaColorSilkscreenResource {
            status: ColorSilkscreenHandoffStatus::RequiresEasyedaProColorSilkscreenExport,
            resource_path,
            mapping_id: mapping.id,
            source_instance_id: source.id,
            treatment_id,
            target: mapping.target,
            asset_id: asset.id,
            original_filename: asset.original_filename.clone(),
            media_type: asset.media_type.clone(),
            asset_sha256: asset.sha256.clone(),
            transform: source.transform,
            crop: image.crop.clone(),
        });
    }
    Ok(resources)
}

fn order_support(
    profile: &ManufacturerProfileSnapshot,
    color_resources: &[EasyedaColorSilkscreenResource],
) -> EasyedaOrderSupport {
    if profile.character_process == CharacterProcess::Multicolor {
        let resource_note = if color_resources.is_empty() {
            "当前工程没有可交接的 colorOriginal PNG/JPEG 映射".to_owned()
        } else {
            format!(
                "handoff manifest 已附带 {} 个逐映射 PNG/JPEG 原图、哈希与放置参数",
                color_resources.len()
            )
        };
        EasyedaOrderSupport {
            status: EasyedaOrderSupportStatus::RequiresManualAdjustment,
            direct_order_supported: false,
            issues: vec![
                format!(
                    "当前 EasyEDA 公共/原生工程适配器不能生成 JLCPCB 专用 FCTS/FCBS 彩色丝印文件；{resource_note}"
                ),
            ],
            downgrade_actions: vec![
                "改用标准丝印（白色或黑色）后重新导出".to_owned(),
                "在 EasyEDA Pro 启用 JLCPCB 彩色丝印工艺，把 manifest 列出的原图按目标面、裁切与物理变换以“原始质量”导入丝印层，再由 EasyEDA Pro 导出彩色丝印生产文件".to_owned(),
            ],
        }
    } else {
        EasyedaOrderSupport {
            status: EasyedaOrderSupportStatus::DirectOrderSupported,
            direct_order_supported: true,
            issues: Vec::new(),
            downgrade_actions: Vec::new(),
        }
    }
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
