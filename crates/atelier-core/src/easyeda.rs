//! Public JLCEDA Professional V3 archive (`.epro2`) foundation.
//!
//! This module consumes only `ResolvedFabricationBoard`: its fixed-grid masks,
//! physical board frame and mechanical features. It never reads source assets,
//! content layers, text, recipes, or preview pixels.
//!
//! The archive layout and line-delimited `DOCHEAD`/`META` protocol were
//! derived from the MIT-licensed PCB_lightgraph Neo implementation at
//! `/Users/tapir/Development/PCB_lightgraph_mac`, notably its `easyeda.rs`.

use std::{
    collections::HashSet,
    fs::File,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::write::SimpleFileOptions;

use crate::{
    BoardOutline, BoardToEasyedaTransform, MechanicalFeature, ProductionTarget,
    ResolvedFabricationBoard, atomic_write_validated, easyeda_paths, polygonize_mask,
};

const FORMAT: &str = "atelier-easyeda-public-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicArchiveExport {
    pub archive_path: PathBuf,
    pub epru_file: String,
    pub board_uuid: String,
    pub pcb_uuid: String,
    pub fill_count: usize,
    pub hole_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicArchiveValidation {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub title: String,
    pub board_uuid: Option<String>,
    pub pcb_uuid: Option<String>,
    pub board_width_um: u32,
    pub board_height_um: u32,
    pub fill_count: usize,
    pub hole_count: usize,
    pub filled_layer_ids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaPreflight {
    pub can_export: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Error)]
pub enum EasyedaPublicError {
    #[error("EasyEDA public archive path has no file name")]
    MissingFileName,
    #[error("EasyEDA public archive contains unsafe entry: {0}")]
    UnsafeArchiveEntry(String),
    #[error("EasyEDA public archive is missing required entry: {0}")]
    MissingEntry(&'static str),
    #[error("EasyEDA public archive has malformed record: {0}")]
    MalformedRecord(String),
    #[error("EasyEDA public archive validation failed: {0}")]
    Validation(String),
    #[error("EasyEDA public archive preflight failed: {0}")]
    Preflight(String),
    #[error("EasyEDA public archive I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("EasyEDA public archive ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("EasyEDA public archive JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn export_public_archive(
    destination: &Path,
    title: &str,
    board: &ResolvedFabricationBoard,
) -> Result<PublicArchiveExport, EasyedaPublicError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(EasyedaPublicError::MalformedRecord(
            "title must not be empty".to_owned(),
        ));
    }
    let preflight = preflight_resolved_board(board);
    if !preflight.can_export {
        return Err(EasyedaPublicError::Preflight(preflight.errors.join("; ")));
    }
    let board_width_um = board.grid.width_um;
    let board_height_um = board.grid.height_um;
    let board_uuid = stable_id(
        "BOARD",
        title,
        board_width_um,
        board_height_um,
        &board.build.output_sha256,
    );
    let pcb_uuid = stable_id(
        "PCB",
        title,
        board_width_um,
        board_height_um,
        &board.build.output_sha256,
    );
    let epru_file = format!("{}.epru", archive_stem(destination)?);
    let (epru, fill_count, hole_count) = build_epru(title, &board_uuid, &pcb_uuid, board)?;
    let archive = build_archive(title, &epru_file, &epru)?;

    atomic_write_validated(
        destination,
        |file| file.write_all(&archive),
        |path| {
            let validation = validate_public_archive(path)?;
            if validation.is_valid {
                Ok(())
            } else {
                Err(EasyedaPublicError::Validation(validation.errors.join("; ")))
            }
        },
    )
    .map_err(map_atomic_error)?;

    Ok(PublicArchiveExport {
        archive_path: destination.to_owned(),
        epru_file,
        board_uuid,
        pcb_uuid,
        fill_count,
        hole_count,
    })
}

pub fn validate_public_archive(path: &Path) -> Result<PublicArchiveValidation, EasyedaPublicError> {
    let (title, epru) = read_public_archive(path)?;
    let mut validation = PublicArchiveValidation {
        is_valid: false,
        errors: Vec::new(),
        title,
        board_uuid: None,
        pcb_uuid: None,
        board_width_um: 0,
        board_height_um: 0,
        fill_count: 0,
        hole_count: 0,
        filled_layer_ids: Vec::new(),
    };
    let records = parse_records(&epru)?;
    let mut board_meta = None;
    let mut pcb_meta = None;
    for pair in records.windows(2) {
        if pair[0].0["type"] != "DOCHEAD" || pair[1].0["type"] != "META" {
            continue;
        }
        match pair[0].1["docType"].as_str() {
            Some("BOARD") => {
                validation.board_uuid = pair[0].1["uuid"].as_str().map(str::to_owned);
                board_meta = Some(&pair[1].1);
            }
            Some("PCB") => {
                validation.pcb_uuid = pair[0].1["uuid"].as_str().map(str::to_owned);
                pcb_meta = Some(&pair[1].1);
            }
            _ => {}
        }
    }
    let Some(board_meta) = board_meta else {
        validation.errors.push("missing BOARD document".to_owned());
        return Ok(validation);
    };
    let Some(pcb_meta) = pcb_meta else {
        validation.errors.push("missing PCB document".to_owned());
        return Ok(validation);
    };
    if board_meta["format"] != FORMAT || pcb_meta["format"] != FORMAT {
        validation
            .errors
            .push("archive format marker is missing".to_owned());
    }
    validation.board_width_um = board_meta["widthUm"].as_u64().unwrap_or_default() as u32;
    validation.board_height_um = board_meta["heightUm"].as_u64().unwrap_or_default() as u32;
    for (head, data) in &records {
        match head["type"].as_str() {
            Some("FILL") => {
                validation.fill_count += 1;
                if let Some(layer_id) = data["layerId"].as_u64().map(|value| value as u32)
                    && !validation.filled_layer_ids.contains(&layer_id)
                {
                    validation.filled_layer_ids.push(layer_id);
                }
            }
            Some("PAD") => validation.hole_count += 1,
            _ => {}
        }
    }
    validation.filled_layer_ids.sort_unstable();
    if validation.board_width_um == 0 || validation.board_height_um == 0 {
        validation
            .errors
            .push("BOARD has invalid physical dimensions".to_owned());
    }
    if pcb_meta["board"] != validation.board_uuid.as_deref().unwrap_or_default() {
        validation
            .errors
            .push("PCB does not reference BOARD".to_owned());
    }
    validation.is_valid = validation.errors.is_empty();
    Ok(validation)
}

pub(crate) fn read_public_archive(path: &Path) -> Result<(String, String), EasyedaPublicError> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;
    let mut epru_names = Vec::new();
    for index in 0..archive.len() {
        let name = archive.by_index(index)?.name().to_owned();
        if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
            return Err(EasyedaPublicError::UnsafeArchiveEntry(name));
        }
        if name.ends_with(".epru") {
            epru_names.push(name);
        }
    }
    if archive.by_name("project2.json").is_err() {
        return Err(EasyedaPublicError::MissingEntry("project2.json"));
    }
    if epru_names.len() != 1 {
        return Err(EasyedaPublicError::MissingEntry("exactly one root .epru"));
    }
    let mut metadata = String::new();
    archive
        .by_name("project2.json")?
        .read_to_string(&mut metadata)?;
    let title = serde_json::from_str::<Value>(&metadata)?["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty())
        .ok_or(EasyedaPublicError::MalformedRecord(
            "project title missing".to_owned(),
        ))?
        .to_owned();
    let mut epru = String::new();
    archive.by_name(&epru_names[0])?.read_to_string(&mut epru)?;
    Ok((title, epru))
}

pub(crate) fn parse_records(epru: &str) -> Result<Vec<(Value, Value)>, EasyedaPublicError> {
    epru.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let line = line.strip_suffix('|').unwrap_or(line);
            let (head, data) = line.split_once("||").ok_or_else(|| {
                EasyedaPublicError::MalformedRecord("record has no || separator".to_owned())
            })?;
            Ok((serde_json::from_str(head)?, serde_json::from_str(data)?))
        })
        .collect()
}

pub fn preflight_resolved_board(board: &ResolvedFabricationBoard) -> EasyedaPreflight {
    let mut errors = Vec::new();
    let expected_layers = [
        "topCopper",
        "topSolderMaskOpen",
        "topSilkscreen",
        "bottomCopper",
        "bottomSolderMaskOpen",
        "bottomSilkscreen",
    ];
    if board.layers.len() != expected_layers.len() {
        errors.push("resolved board must contain exactly six production layers".to_owned());
    }
    let actual_layers = board
        .layers
        .iter()
        .map(|layer| layer.target.canonical_name())
        .collect::<HashSet<_>>();
    for expected in expected_layers {
        if !actual_layers.contains(expected) {
            errors.push(format!("resolved board is missing {expected}"));
        }
    }
    for layer in &board.layers {
        if layer.composite.width_px() != board.grid.width_px
            || layer.composite.height_px() != board.grid.height_px
        {
            errors.push(format!(
                "{} mask dimensions do not match grid",
                layer.target.canonical_name()
            ));
        }
        if layer.composite_sha256 != layer.composite.sha256() {
            errors.push(format!(
                "{} mask checksum does not match composite",
                layer.target.canonical_name()
            ));
        }
    }
    EasyedaPreflight {
        can_export: errors.is_empty(),
        errors,
    }
}

fn build_epru(
    title: &str,
    board_uuid: &str,
    pcb_uuid: &str,
    board: &ResolvedFabricationBoard,
) -> Result<(String, usize, usize), EasyedaPublicError> {
    let mut records = Vec::new();
    let mut ticket = 1_u64;
    let mut primitive_id = 1_u64;
    push_record(
        &mut records,
        json!({"type":"DOCHEAD", "ticket": ticket}),
        json!({"uuid":board_uuid,"docType":"BOARD","version":"atelier-5.2","updateTime":0}),
    )?;
    ticket += 1;
    push_record(
        &mut records,
        json!({"type":"META", "ticket": ticket}),
        json!({"format":FORMAT,"title":format!("{title} board"),"zIndex":0,"widthUm":board.grid.width_um,"heightUm":board.grid.height_um}),
    )?;
    ticket += 1;
    push_record(
        &mut records,
        json!({"type":"DOCHEAD", "ticket": ticket}),
        json!({"uuid":pcb_uuid,"docType":"PCB","version":"atelier-5.2","updateTime":0}),
    )?;
    ticket += 1;
    push_record(
        &mut records,
        json!({"type":"META", "ticket": ticket}),
        json!({"format":FORMAT,"title":title,"board":board_uuid,"zIndex":1,"widthUm":board.grid.width_um,"heightUm":board.grid.height_um,"artworkPrimitives":"static-fill"}),
    )?;
    ticket += 1;
    push_record(
        &mut records,
        json!({"type":"POLY", "ticket": ticket, "id":format!("e{primitive_id}")}),
        json!({"layerId":11,"width":3.937,"path":outline_path(&board.outline),"locked":false,"zIndex":1,"polyType":"BOARD_OUTLINE"}),
    )?;
    ticket += 1;
    primitive_id += 1;
    let mut fill_count = 0;
    for layer in &board.layers {
        let layer_id = easyeda_layer_id(layer.target);
        for fill in polygonize_mask(&layer.composite, &board.grid)
            .map_err(|error| EasyedaPublicError::MalformedRecord(error.to_string()))?
        {
            // The document stores both faces in physical board coordinates.
            // Do not mirror the bottom side: JLCEDA controls bottom-view
            // mirroring; this adapter only changes the raster Y origin.
            let paths = easyeda_paths(&fill, &board.grid, BoardToEasyedaTransform::default())
                .map_err(|error| EasyedaPublicError::MalformedRecord(error.to_string()))?
                .into_iter()
                .map(|path| linear_easyeda_path(path.points_mil))
                .collect::<Vec<_>>();
            push_record(
                &mut records,
                json!({"type":"FILL", "ticket":ticket, "id":format!("e{primitive_id}")}),
                json!({"partitionId":"","groupId":0,"netName":"","layerId":layer_id,"width":0.2,"fillStyle":"SOLID","path":paths,"locked":false,"zIndex":primitive_id+1,"isBridgingCopper":false,"networkList":[],"refs":[]}),
            )?;
            ticket += 1;
            primitive_id += 1;
            fill_count += 1;
        }
    }
    for (index, feature) in board.mechanical_features.iter().enumerate() {
        let (x_um, y_um, drill_um, plated, pad_um) = match feature {
            MechanicalFeature::NpthRound {
                center_x_um,
                center_y_um,
                diameter_um,
            } => (
                *center_x_um,
                *center_y_um,
                *diameter_um,
                false,
                *diameter_um,
            ),
            MechanicalFeature::PthRound {
                center_x_um,
                center_y_um,
                drill_um,
                pad_um,
                ..
            } => (*center_x_um, *center_y_um, *drill_um, true, *pad_um),
        };
        let center_x = um_to_mil(x_um);
        let center_y = um_to_mil(i64::from(board.grid.height_um) - y_um);
        let drill = um_to_mil(i64::from(drill_um));
        let pad = um_to_mil(i64::from(pad_um));
        push_record(
            &mut records,
            json!({"type":"PAD", "ticket":ticket, "id":format!("e{primitive_id}")}),
            json!({"layerId":12,"num":(index+1).to_string(),"centerX":center_x,"centerY":center_y,"padAngle":0,"hole":{"holeType":"ROUND","width":drill,"height":drill},"defaultPad":{"padType":"ELLIPSE","width":pad,"height":pad},"plated":plated,"padType":"NORMAL","locked":false,"zIndex":primitive_id+1}),
        )?;
        ticket += 1;
        primitive_id += 1;
    }
    Ok((
        records.join("\n"),
        fill_count,
        board.mechanical_features.len(),
    ))
}

fn build_archive(title: &str, epru_name: &str, epru: &str) -> Result<Vec<u8>, EasyedaPublicError> {
    let mut cursor = Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(&mut cursor);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    archive.start_file("project2.json", options)?;
    archive.write_all(
        serde_json::to_string_pretty(&json!({
            "title": title,
            "introduction": "PCB Atelier public EDA handoff with resolved static artwork"
        }))?
        .as_bytes(),
    )?;
    archive.start_file(epru_name, options)?;
    archive.write_all(epru.as_bytes())?;
    archive.finish()?;
    Ok(cursor.into_inner())
}

fn archive_stem(path: &Path) -> Result<String, EasyedaPublicError> {
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or(EasyedaPublicError::MissingFileName)?;
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        return Err(EasyedaPublicError::MissingFileName);
    }
    Ok(sanitized.to_owned())
}

fn stable_id(
    kind: &str,
    title: &str,
    width_um: u32,
    height_um: u32,
    output_sha256: &str,
) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!("{kind}\0{title}\0{width_um}\0{height_um}\0{output_sha256}").as_bytes()
        )
    )[..32]
        .to_owned()
}

fn push_record(
    records: &mut Vec<String>,
    head: Value,
    data: Value,
) -> Result<(), EasyedaPublicError> {
    records.push(format!(
        "{}||{}|",
        serde_json::to_string(&head)?,
        serde_json::to_string(&data)?
    ));
    Ok(())
}

fn outline_path(outline: &BoardOutline) -> Vec<Value> {
    let (width_um, height_um, radius_um) = match outline {
        BoardOutline::Rectangle {
            width_um,
            height_um,
        } => (*width_um, *height_um, 0),
        BoardOutline::RoundedRectangle {
            width_um,
            height_um,
            corner_radius_um,
        } => (*width_um, *height_um, *corner_radius_um),
    };
    vec![
        json!("R"),
        json!(0.0),
        json!(um_to_mil(i64::from(height_um))),
        json!(um_to_mil(i64::from(width_um))),
        json!(um_to_mil(i64::from(height_um))),
        json!(0.0),
        json!(um_to_mil(i64::from(radius_um))),
    ]
}

fn linear_easyeda_path(points_mil: Vec<(f64, f64)>) -> Value {
    let mut path = Vec::with_capacity(points_mil.len() * 2 + 1);
    if let Some(((first_x, first_y), tail)) = points_mil.split_first() {
        path.push(json!(first_x));
        path.push(json!(first_y));
        path.push(json!("L"));
        for (x, y) in tail {
            path.push(json!(x));
            path.push(json!(y));
        }
    }
    Value::Array(path)
}

fn easyeda_layer_id(target: ProductionTarget) -> u32 {
    match target.canonical_name() {
        "topCopper" => 1,
        "bottomCopper" => 2,
        "topSilkscreen" => 3,
        "bottomSilkscreen" => 4,
        "topSolderMaskOpen" => 5,
        "bottomSolderMaskOpen" => 6,
        _ => unreachable!("production targets are closed"),
    }
}

fn um_to_mil(value_um: i64) -> f64 {
    value_um as f64 / 25.4
}

fn map_atomic_error(
    error: crate::AtomicWriteError<std::io::Error, EasyedaPublicError>,
) -> EasyedaPublicError {
    match error {
        crate::AtomicWriteError::Io(error) | crate::AtomicWriteError::Write(error) => {
            EasyedaPublicError::Io(error)
        }
        crate::AtomicWriteError::Validation(error) => error,
        crate::AtomicWriteError::Persist(error) => EasyedaPublicError::Io(error.error),
    }
}
