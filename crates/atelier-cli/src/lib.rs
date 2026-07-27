use std::path::{Path, PathBuf};

use atelier_core::{
    AssetId, AtelierDocument, CommandError, CommandHistory, CompiledImageTreatment,
    DocumentCommand, EasyedaHandoffError, FabricationError, FabricationResolveError,
    ManufacturerProfileSnapshot, ProductionCoordinateSpace, ProductionLayerTrace,
    ProductionOperationTrace, ProjectBundle, ProjectBundleRasterizer, ProjectError,
    ResolvedFabricationBoard, SamplingPurpose, TreatmentCompileError, TreatmentCompileRequest,
    TreatmentId, build_production_trace, compile_fabrication_plan, compile_image_treatment,
    export_easyeda_handoff, resolve_fabrication_plan, resolve_fabrication_plan_for_purpose,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub fn execute(args: &[String]) -> Result<String, CliError> {
    let (command, rest) = args.split_first().ok_or(CliError::MissingCommand)?;
    match command.as_str() {
        "new" => create_project(rest),
        "apply" => apply_commands(rest),
        "validate" => validate_project(rest),
        "asset-import" => import_asset(rest),
        "treatment-compile" => compile_treatment(rest),
        "manufacturer-validate" => validate_manufacturer(rest),
        "production-inspect" => production_inspect(rest),
        "export-easyeda" => export_easyeda(rest),
        other => Err(CliError::UnknownCommand(other.to_owned())),
    }
}

fn import_asset(args: &[String]) -> Result<String, CliError> {
    if args.len() < 2 {
        return Err(CliError::Usage(
            "asset-import <project.pcba> <source-image> --media-type <type> --pixel-width <positive integer> --pixel-height <positive integer> [--output <path>]"
                .to_owned(),
        ));
    }
    let project_path = PathBuf::from(&args[0]);
    let source_path = PathBuf::from(&args[1]);
    let mut media_type = None;
    let mut pixel_width = None;
    let mut pixel_height = None;
    let mut output_path = project_path.clone();
    let mut index = 2;
    while index < args.len() {
        let flag = &args[index];
        let value = args
            .get(index + 1)
            .ok_or_else(|| CliError::MissingOptionValue(flag.clone()))?;
        match flag.as_str() {
            "--media-type" => media_type = Some(value.clone()),
            "--pixel-width" => pixel_width = Some(parse_positive_u32(value, flag)?),
            "--pixel-height" => pixel_height = Some(parse_positive_u32(value, flag)?),
            "--output" => output_path = PathBuf::from(value),
            _ => return Err(CliError::UnknownOption(flag.clone())),
        }
        index += 2;
    }
    let media_type = media_type.ok_or(CliError::MissingArgument("--media-type"))?;
    let pixel_width = pixel_width.ok_or(CliError::MissingArgument("--pixel-width"))?;
    let pixel_height = pixel_height.ok_or(CliError::MissingArgument("--pixel-height"))?;
    let bytes = std::fs::read(&source_path)?;
    let filename = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CliError::InvalidSourcePath(source_path.clone()))?;
    let mut bundle = ProjectBundle::open(&project_path)?;
    let previous_count = bundle.document.assets.len();
    let asset_id = bundle.embed_asset(filename, media_type, pixel_width, pixel_height, bytes)?;
    let asset = bundle
        .document
        .assets
        .iter()
        .find(|asset| asset.id == asset_id)
        .expect("embedded asset reference");
    let report = AssetImportReport {
        asset_id,
        sha256: asset.sha256.clone(),
        reused: bundle.document.assets.len() == previous_count,
        output_path: output_path.clone(),
    };
    bundle.save(&output_path)?;
    serde_json::to_string(&report).map_err(CliError::Json)
}

fn compile_treatment(args: &[String]) -> Result<String, CliError> {
    if args.len() < 2 {
        return Err(CliError::Usage(
            "treatment-compile <project.pcba> <treatment-id> --width-um <positive integer> --height-um <positive integer> [--purpose interactive-proxy|board-preview|formal-production] [--pitch-um <positive integer>]"
                .to_owned(),
        ));
    }
    let project_path = PathBuf::from(&args[0]);
    let treatment_id: TreatmentId = parse_uuid_id(&args[1], "treatment-id")?;
    let mut width_um = None;
    let mut height_um = None;
    let mut purpose = SamplingPurpose::FormalProduction;
    let mut pitch_um = None;
    let mut index = 2;
    while index < args.len() {
        let flag = &args[index];
        let value = args
            .get(index + 1)
            .ok_or_else(|| CliError::MissingOptionValue(flag.clone()))?;
        match flag.as_str() {
            "--width-um" => width_um = Some(parse_positive_u32(value, flag)?),
            "--height-um" => height_um = Some(parse_positive_u32(value, flag)?),
            "--pitch-um" => pitch_um = Some(parse_positive_u32(value, flag)?),
            "--purpose" => {
                purpose = match value.as_str() {
                    "interactive-proxy" => SamplingPurpose::InteractiveProxy,
                    "board-preview" => SamplingPurpose::BoardPreview,
                    "formal-production" => SamplingPurpose::FormalProduction,
                    _ => return Err(CliError::InvalidPurpose(value.clone())),
                }
            }
            _ => return Err(CliError::UnknownOption(flag.clone())),
        }
        index += 2;
    }
    let bundle = ProjectBundle::open(&project_path)?;
    let treatment = bundle
        .document
        .image_treatments
        .iter()
        .find(|treatment| treatment.id == treatment_id)
        .ok_or(CliError::MissingTreatment(treatment_id))?;
    let bytes = bundle
        .asset_bytes(treatment.asset_id)
        .ok_or(ProjectError::MissingAsset(treatment.asset_id))?;
    let request = TreatmentCompileRequest {
        physical_width_um: width_um.ok_or(CliError::MissingArgument("--width-um"))?,
        physical_height_um: height_um.ok_or(CliError::MissingArgument("--height-um"))?,
        pixel_pitch_um: pitch_um.unwrap_or_else(|| purpose.default_pixel_pitch_um()),
        revision: 0,
        purpose,
    };
    let compiled = compile_image_treatment(bytes, &treatment.recipe, request)?;
    serde_json::to_string(&TreatmentCompileReport::from_compiled(
        treatment_id,
        &compiled,
    ))
    .map_err(CliError::Json)
}

fn validate_manufacturer(args: &[String]) -> Result<String, CliError> {
    if args.len() != 1 {
        return Err(CliError::Usage(
            "manufacturer-validate <project.pcba>".to_owned(),
        ));
    }
    let bundle = ProjectBundle::open(Path::new(&args[0]))?;
    let profile = bundle.document.manufacturer_profile;
    let errors = profile.validate().err().unwrap_or_default();
    serde_json::to_string(&ManufacturerValidationReport {
        profile,
        valid: errors.is_empty(),
        errors,
    })
    .map_err(CliError::Json)
}

fn create_project(args: &[String]) -> Result<String, CliError> {
    let (output, options) = args
        .split_first()
        .ok_or(CliError::MissingArgument("output"))?;
    let mut title = "未命名卡片".to_owned();
    let mut width_um = None;
    let mut height_um = None;
    let mut index = 0;
    while index < options.len() {
        let flag = &options[index];
        let value = options
            .get(index + 1)
            .ok_or_else(|| CliError::MissingOptionValue(flag.clone()))?;
        match flag.as_str() {
            "--title" => title = value.clone(),
            "--width-mm" => width_um = Some(parse_mm(value)?),
            "--height-mm" => height_um = Some(parse_mm(value)?),
            _ => return Err(CliError::UnknownOption(flag.clone())),
        }
        index += 2;
    }
    let width_um = width_um.ok_or(CliError::MissingArgument("--width-mm"))?;
    let height_um = height_um.ok_or(CliError::MissingArgument("--height-mm"))?;
    let document = AtelierDocument::new_card(title, width_um, height_um);
    ProjectBundle::new(document).save(Path::new(output))?;
    Ok(format!("已创建卡片工程：{output}"))
}

fn apply_commands(args: &[String]) -> Result<String, CliError> {
    if args.len() < 2 {
        return Err(CliError::Usage(
            "apply <project.pcba> <commands.json> [--output <path>]".to_owned(),
        ));
    }
    let project_path = PathBuf::from(&args[0]);
    let commands_path = PathBuf::from(&args[1]);
    let output_path = match args.get(2).map(String::as_str) {
        None => project_path.clone(),
        Some("--output") => PathBuf::from(
            args.get(3)
                .ok_or_else(|| CliError::MissingOptionValue("--output".to_owned()))?,
        ),
        Some(option) => return Err(CliError::UnknownOption(option.to_owned())),
    };
    if args.len() > 4 {
        return Err(CliError::Usage(
            "apply accepts only one optional --output path".to_owned(),
        ));
    }

    let mut bundle = ProjectBundle::open(&project_path)?;
    let commands = parse_commands(&std::fs::read(&commands_path)?)?;
    let command_count = commands.len();
    let mut history = CommandHistory::default();
    for command in commands {
        history.execute(&mut bundle.document, command)?;
    }
    bundle.save(&output_path)?;
    Ok(format!(
        "已应用 {command_count} 条命令：{}",
        output_path.display()
    ))
}

fn validate_project(args: &[String]) -> Result<String, CliError> {
    if args.len() != 1 {
        return Err(CliError::Usage("validate <project.pcba>".to_owned()));
    }
    ProjectBundle::open(Path::new(&args[0]))?;
    Ok(format!("工程有效：{}", args[0]))
}

fn production_inspect(args: &[String]) -> Result<String, CliError> {
    let (project_path, pitch_um) = parse_project_and_pitch(
        args,
        "production-inspect <project.pcba> [--pitch-um <positive integer>]",
    )?;
    let bundle = ProjectBundle::open(&project_path)?;
    let document_sha256 = document_sha256(&bundle.document)?;
    let board = resolve_bundle(&bundle, pitch_um)?;
    serde_json::to_string(&ProductionInspectReport::from_board(
        document_sha256,
        &bundle.document,
        &board,
    ))
    .map_err(CliError::Json)
}

fn export_easyeda(args: &[String]) -> Result<String, CliError> {
    if args.len() < 2 {
        return Err(CliError::Usage(
            "export-easyeda <project.pcba> <output-directory> [--pitch-um <positive integer>]"
                .to_owned(),
        ));
    }
    let project_path = PathBuf::from(&args[0]);
    let destination = PathBuf::from(&args[1]);
    let pitch_um = parse_pitch_options(
        &args[2..],
        "export-easyeda <project.pcba> <output-directory> [--pitch-um <positive integer>]",
    )?;
    let bundle = ProjectBundle::open(&project_path)?;
    let formal_pitch_um = SamplingPurpose::FormalProduction.default_pixel_pitch_um();
    if pitch_um != formal_pitch_um {
        return Err(EasyedaHandoffError::NonFormalProduction {
            required_pitch_um: formal_pitch_um,
            actual_pitch_um: pitch_um,
            actual_purpose: None,
        }
        .into());
    }
    let plan = compile_fabrication_plan(&bundle.document)?;
    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).map_err(CliError::Rasterizer)?;
    let board = resolve_fabrication_plan_for_purpose(
        &plan,
        SamplingPurpose::FormalProduction,
        &mut rasterizer,
    )?;
    let report = export_easyeda_handoff(&destination, &bundle, &board)?;
    serde_json::to_string(&report).map_err(CliError::Json)
}

fn resolve_bundle(
    bundle: &ProjectBundle,
    pitch_um: u32,
) -> Result<ResolvedFabricationBoard, CliError> {
    let plan = compile_fabrication_plan(&bundle.document)?;
    let mut rasterizer = ProjectBundleRasterizer::new(bundle).map_err(CliError::Rasterizer)?;
    Ok(resolve_fabrication_plan(&plan, pitch_um, &mut rasterizer)?)
}

fn parse_project_and_pitch(
    args: &[String],
    usage: &'static str,
) -> Result<(PathBuf, u32), CliError> {
    let (project, options) = args
        .split_first()
        .ok_or_else(|| CliError::Usage(usage.to_owned()))?;
    Ok((PathBuf::from(project), parse_pitch_options(options, usage)?))
}

fn parse_pitch_options(args: &[String], usage: &'static str) -> Result<u32, CliError> {
    match args {
        [] => Ok(25),
        [flag, value] if flag == "--pitch-um" => value
            .parse::<u32>()
            .ok()
            .filter(|pitch| *pitch > 0)
            .ok_or_else(|| CliError::InvalidPitch(value.clone())),
        [flag, ..] if flag == "--pitch-um" => Err(CliError::MissingOptionValue(flag.clone())),
        [option, ..] => Err(CliError::UnknownOption(option.clone())),
    }
    .map_err(|error| match error {
        CliError::UnknownOption(_)
        | CliError::MissingOptionValue(_)
        | CliError::InvalidPitch(_) => error,
        _ => CliError::Usage(usage.to_owned()),
    })
}

fn document_sha256(document: &AtelierDocument) -> Result<String, CliError> {
    let canonical = serde_json::to_vec(document)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductionInspectReport {
    format: &'static str,
    document_sha256: String,
    board: BoardDimensions,
    pixel_pitch_um: u32,
    coordinate_space: ProductionCoordinateSpace,
    layers: Vec<ProductionLayerTrace>,
    build: ProductionBuildInspection,
    manufacturer_profile: ManufacturerProfileSnapshot,
    manufacturer_profile_fingerprint: String,
    operations: Vec<ProductionOperationTrace>,
}

impl ProductionInspectReport {
    fn from_board(
        document_sha256: String,
        document: &AtelierDocument,
        board: &ResolvedFabricationBoard,
    ) -> Self {
        let trace = build_production_trace(0, document, board);
        let atelier_core::ProductionTraceReport {
            coordinate_space,
            manufacturer_profile,
            manufacturer_profile_fingerprint,
            layers,
            operations,
            ..
        } = trace;
        Self {
            format: "atelier-production-inspect-v1",
            document_sha256,
            board: BoardDimensions {
                width_um: board.grid.width_um,
                height_um: board.grid.height_um,
            },
            pixel_pitch_um: board.grid.pixel_pitch_um,
            coordinate_space,
            layers,
            build: ProductionBuildInspection {
                input_sha256: board.build.input_sha256.clone(),
                output_sha256: board.build.output_sha256.clone(),
            },
            manufacturer_profile,
            manufacturer_profile_fingerprint,
            operations,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardDimensions {
    width_um: u32,
    height_um: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductionBuildInspection {
    input_sha256: String,
    output_sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetImportReport {
    asset_id: AssetId,
    sha256: String,
    reused: bool,
    output_path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TreatmentCompileReport {
    treatment_id: TreatmentId,
    width_px: u32,
    height_px: u32,
    mask_sha256: String,
    pixel_pitch_um: u32,
    recipe_fingerprint: String,
    revision: u64,
    purpose: SamplingPurpose,
    topology: atelier_core::MaskTopology,
    diagnostics: Vec<atelier_core::TreatmentDiagnostic>,
}

impl TreatmentCompileReport {
    fn from_compiled(treatment_id: TreatmentId, compiled: &CompiledImageTreatment) -> Self {
        Self {
            treatment_id,
            width_px: compiled.mask.width_px(),
            height_px: compiled.mask.height_px(),
            mask_sha256: compiled.mask.sha256(),
            pixel_pitch_um: compiled.pixel_pitch_um,
            recipe_fingerprint: compiled.recipe_fingerprint.clone(),
            revision: compiled.revision,
            purpose: compiled.purpose,
            topology: compiled.topology,
            diagnostics: compiled.diagnostics.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManufacturerValidationReport {
    profile: ManufacturerProfileSnapshot,
    valid: bool,
    errors: Vec<String>,
}

fn parse_commands(bytes: &[u8]) -> Result<Vec<DocumentCommand>, CliError> {
    let value: Value = serde_json::from_slice(bytes)?;
    if value.is_array() {
        Ok(serde_json::from_value(value)?)
    } else {
        Ok(vec![serde_json::from_value(value)?])
    }
}

fn parse_positive_u32(value: &str, flag: &str) -> Result<u32, CliError> {
    value
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| CliError::InvalidPositiveInteger {
            flag: flag.to_owned(),
            value: value.to_owned(),
        })
}

fn parse_uuid_id<T: serde::de::DeserializeOwned>(
    value: &str,
    label: &'static str,
) -> Result<T, CliError> {
    serde_json::from_value(Value::String(value.to_owned())).map_err(|_| CliError::InvalidId {
        label,
        value: value.to_owned(),
    })
}

fn parse_mm(value: &str) -> Result<u32, CliError> {
    if value.starts_with('-') || value.starts_with('+') || value.is_empty() {
        return Err(CliError::InvalidDimension(value.to_owned()));
    }
    let mut parts = value.split('.');
    let whole = parts
        .next()
        .ok_or_else(|| CliError::InvalidDimension(value.to_owned()))?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.chars().all(|character| character.is_ascii_digit())
        || !fraction.chars().all(|character| character.is_ascii_digit())
    {
        return Err(CliError::InvalidDimension(value.to_owned()));
    }
    if fraction.len() > 3 && fraction[3..].chars().any(|character| character != '0') {
        return Err(CliError::SubMicrometreDimension(value.to_owned()));
    }
    let whole_um = whole
        .parse::<u64>()
        .map_err(|_| CliError::InvalidDimension(value.to_owned()))?
        .checked_mul(1_000)
        .ok_or_else(|| CliError::DimensionOverflow(value.to_owned()))?;
    let mut fraction_um = fraction.chars().take(3).collect::<String>();
    while fraction_um.len() < 3 {
        fraction_um.push('0');
    }
    let fraction_um = if fraction_um.is_empty() {
        0
    } else {
        fraction_um
            .parse::<u64>()
            .map_err(|_| CliError::InvalidDimension(value.to_owned()))?
    };
    let result = whole_um
        .checked_add(fraction_um)
        .ok_or_else(|| CliError::DimensionOverflow(value.to_owned()))?;
    u32::try_from(result).map_err(|_| CliError::DimensionOverflow(value.to_owned()))
}

#[derive(Debug, Error)]
pub enum CliError {
    #[error(
        "缺少命令；可用命令：new、apply、validate、asset-import、treatment-compile、manufacturer-validate、production-inspect、export-easyeda"
    )]
    MissingCommand,
    #[error("未知命令：{0}")]
    UnknownCommand(String),
    #[error("缺少参数：{0}")]
    MissingArgument(&'static str),
    #[error("选项缺少值：{0}")]
    MissingOptionValue(String),
    #[error("未知选项：{0}")]
    UnknownOption(String),
    #[error("用法错误：{0}")]
    Usage(String),
    #[error("无效毫米尺寸：{0}")]
    InvalidDimension(String),
    #[error("尺寸精度小于 one micrometre，无法无损表示：{0}")]
    SubMicrometreDimension(String),
    #[error("尺寸超出可表示范围：{0}")]
    DimensionOverflow(String),
    #[error("无效生产像素间距（必须为正整数微米）：{0}")]
    InvalidPitch(String),
    #[error("无效正整数选项 {flag}：{value}")]
    InvalidPositiveInteger { flag: String, value: String },
    #[error("无效 {label}：{value}")]
    InvalidId { label: &'static str, value: String },
    #[error("无效素材源路径：{0:?}")]
    InvalidSourcePath(PathBuf),
    #[error("无效处理编译用途：{0}")]
    InvalidPurpose(String),
    #[error("找不到图片处理版本：{0}")]
    MissingTreatment(TreatmentId),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Fabrication(#[from] FabricationError),
    #[error(transparent)]
    FabricationResolve(#[from] FabricationResolveError),
    #[error("生产栅格器初始化失败：{0}")]
    Rasterizer(String),
    #[error(transparent)]
    TreatmentCompile(#[from] TreatmentCompileError),
    #[error(transparent)]
    EasyedaHandoff(#[from] EasyedaHandoffError),
    #[error("JSON 错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O 错误：{0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::parse_mm;

    #[test]
    fn millimetres_convert_to_integer_micrometres_without_floating_point() {
        assert_eq!(parse_mm("64").unwrap(), 64_000);
        assert_eq!(parse_mm("0.025").unwrap(), 25);
        assert_eq!(parse_mm("100.5000").unwrap(), 100_500);
        assert!(parse_mm("0.0001").is_err());
    }
}
