use std::path::{Path, PathBuf};

use atelier_core::{
    AtelierDocument, CommandError, CommandHistory, DocumentCommand, EasyedaHandoffError,
    FabricationError, FabricationResolveError, ProjectBundle, ProjectBundleRasterizer,
    ProjectError, ResolvedFabricationBoard, compile_fabrication_plan, export_easyeda_handoff,
    resolve_fabrication_plan,
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
        "production-inspect" => production_inspect(rest),
        "export-easyeda" => export_easyeda(rest),
        other => Err(CliError::UnknownCommand(other.to_owned())),
    }
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
    let board = resolve_bundle(&bundle, pitch_um)?;
    let report = export_easyeda_handoff(&destination, &bundle.document.title, &board)?;
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
    layers: Vec<ProductionLayerInspection>,
    build: ProductionBuildInspection,
}

impl ProductionInspectReport {
    fn from_board(document_sha256: String, board: &ResolvedFabricationBoard) -> Self {
        Self {
            format: "atelier-production-inspect-v1",
            document_sha256,
            board: BoardDimensions {
                width_um: board.grid.width_um,
                height_um: board.grid.height_um,
            },
            pixel_pitch_um: board.grid.pixel_pitch_um,
            layers: board
                .layers
                .iter()
                .map(|layer| ProductionLayerInspection {
                    target: layer.target.canonical_name(),
                    polarity: layer.polarity,
                    composite_sha256: layer.composite_sha256.clone(),
                })
                .collect(),
            build: ProductionBuildInspection {
                input_sha256: board.build.input_sha256.clone(),
                output_sha256: board.build.output_sha256.clone(),
            },
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
struct ProductionLayerInspection {
    target: &'static str,
    polarity: atelier_core::LayerPolarity,
    composite_sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductionBuildInspection {
    input_sha256: String,
    output_sha256: String,
}

fn parse_commands(bytes: &[u8]) -> Result<Vec<DocumentCommand>, CliError> {
    let value: Value = serde_json::from_slice(bytes)?;
    if value.is_array() {
        Ok(serde_json::from_value(value)?)
    } else {
        Ok(vec![serde_json::from_value(value)?])
    }
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
    #[error("缺少命令；可用命令：new、apply、validate、production-inspect、export-easyeda")]
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
