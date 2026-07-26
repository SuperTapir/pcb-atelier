//! Native JLCEDA Professional project (`.eprj2`) conversion and validation.
//!
//! Derived from the MIT-licensed PCB_lightgraph Neo implementation at
//! `/Users/tapir/Development/PCB_lightgraph_mac/crates/neo-core/src/easyeda_native.rs`.
//! This module deliberately accepts only a public `.epro2` archive; it has no
//! dependency on Neo's LayerSet, recipe, preview, or UI models. Task 5.2 supplies
//! static artwork primitives through the public archive.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use aes_gcm::{
    AesGcm, Nonce,
    aead::{Aead, KeyInit, consts::U16},
    aes::Aes128,
};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use rand::RngCore;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

const SQLITE_SCHEMA_VERSION: &str = "25.10.31.1";
const NATIVE_TABLE_COUNT: i64 = 35;
const NATIVE_INDEX_COUNT: i64 = 49;
type Aes128GcmWith16ByteNonce = AesGcm<Aes128, U16>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaNativeLayerExtent {
    pub layer_id: u32,
    /// EasyEDA mil coordinates multiplied by 1,000,000 to preserve exact
    /// regression-testable direction without serializing floating point state.
    pub min_x_nano_mil: i64,
    pub max_x_nano_mil: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaNativeValidation {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub project_uuid: Option<String>,
    pub branch_uuid: Option<String>,
    pub history_uuid: Option<String>,
    pub board_uuids: Vec<String>,
    pub pcb_uuids: Vec<String>,
    pub payload_records: usize,
    pub table_count: i64,
    pub index_count: i64,
    pub board_width_um: u32,
    pub board_height_um: u32,
    pub fill_count: usize,
    pub hole_count: usize,
    pub filled_layer_ids: Vec<u32>,
    /// Layer IDs 5 and 6 represent solder-mask *openings*, never a mask fill.
    pub solder_mask_opening_layer_ids: Vec<u32>,
    pub layer_x_extents: Vec<EasyedaNativeLayerExtent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaNativeExportResult {
    pub project_path: PathBuf,
    pub source_path: PathBuf,
    pub title: String,
    pub project_uuid: String,
    pub branch_uuid: String,
    pub history_uuid: String,
    pub board_uuid: String,
    pub pcb_uuid: String,
    pub validation: EasyedaNativeValidation,
}

#[derive(Debug, Error)]
pub enum EasyedaNativeError {
    #[error("EasyEDA native project path has no parent directory")]
    MissingParent,
    #[error("EasyEDA public archive is missing project2.json")]
    MissingProjectMetadata,
    #[error("EasyEDA public archive must contain exactly one root .epru file")]
    MissingEpru,
    #[error("unsafe EasyEDA archive entry: {0}")]
    UnsafeArchiveEntry(String),
    #[error("EasyEDA public archive metadata has no title")]
    MissingTitle,
    #[error("EasyEDA history payload has no {0} document")]
    MissingDocument(&'static str),
    #[error("EasyEDA history record is malformed: {0}")]
    MalformedRecord(String),
    #[error("EasyEDA native project validation failed: {0}")]
    Validation(String),
    #[error("EasyEDA native project I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("EasyEDA native project ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("EasyEDA native project JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("EasyEDA native project SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not atomically replace EasyEDA native project: {0}")]
    Persist(#[from] tempfile::PathPersistError),
    #[error("EasyEDA native history encryption failed")]
    Encryption,
}

#[derive(Debug)]
struct PublicArchive {
    title: String,
    epru: String,
}

#[derive(Debug, Clone)]
struct Document {
    uuid: String,
    doc_type: String,
    version: String,
    update_time: u64,
    metadata: Map<String, Value>,
}

#[derive(Debug)]
struct ParsedDocuments {
    boards: BTreeMap<String, Document>,
    pcbs: BTreeMap<String, Document>,
    record_count: usize,
}

#[derive(Debug)]
struct NativeIds {
    owner: String,
    start_branch: String,
    main_branch: String,
    history: String,
    history_key: [u8; 16],
}

/// Convert a public JLCEDA Professional V3 `.epro2` archive into a directly
/// openable native `.eprj2` SQLite project.
pub fn convert_easyeda_archive_to_native(
    source: &Path,
    destination: &Path,
) -> Result<EasyedaNativeExportResult, EasyedaNativeError> {
    let archive = read_public_archive(source)?;
    let documents = parse_documents(&archive.epru)?;
    let board_uuid = documents
        .boards
        .keys()
        .next()
        .cloned()
        .ok_or(EasyedaNativeError::MissingDocument("BOARD"))?;
    let pcb_uuid = documents
        .pcbs
        .keys()
        .next()
        .cloned()
        .ok_or(EasyedaNativeError::MissingDocument("PCB"))?;
    let absolute_destination = absolute_output_path(destination)?;
    let project_uuid = sha256_hex(absolute_destination.to_string_lossy().as_bytes());
    let ids = NativeIds {
        owner: random_uuid(),
        start_branch: random_uuid(),
        main_branch: random_uuid(),
        history: random_uuid(),
        history_key: random_bytes(),
    };
    let structure = build_project_structure(&documents, &ids.owner);
    let payload = build_history_payload(&archive.epru, &ids.owner)?;
    let encrypted_payload = encrypt_history(&payload, &ids.history, &ids.history_key)?;
    let encoded_payload = base64_encode(&encrypted_payload);

    let parent = destination
        .parent()
        .ok_or(EasyedaNativeError::MissingParent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?.into_temp_path();
    write_native_database(
        &temporary,
        &archive.title,
        &project_uuid,
        &ids,
        &structure,
        &encoded_payload,
    )?;
    let validation = validate_easyeda_native_project_with_identity(&temporary, destination)?;
    if !validation.is_valid {
        return Err(EasyedaNativeError::Validation(validation.errors.join("; ")));
    }
    temporary.persist(destination)?;

    Ok(EasyedaNativeExportResult {
        project_path: destination.to_owned(),
        source_path: source.to_owned(),
        title: archive.title,
        project_uuid,
        branch_uuid: ids.main_branch,
        history_uuid: ids.history,
        board_uuid,
        pcb_uuid,
        validation,
    })
}

/// Validate the native SQLite schema, decrypt the history payload, and verify
/// the project/branch/history/document identifier graph.
pub fn validate_easyeda_native_project(
    path: &Path,
) -> Result<EasyedaNativeValidation, EasyedaNativeError> {
    validate_easyeda_native_project_with_identity(path, path)
}

fn validate_easyeda_native_project_with_identity(
    path: &Path,
    identity_path: &Path,
) -> Result<EasyedaNativeValidation, EasyedaNativeError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut validation = EasyedaNativeValidation {
        is_valid: false,
        errors: Vec::new(),
        project_uuid: None,
        branch_uuid: None,
        history_uuid: None,
        board_uuids: Vec::new(),
        pcb_uuids: Vec::new(),
        payload_records: 0,
        table_count: schema_count(&connection, "table")?,
        index_count: schema_count(&connection, "index")?,
        board_width_um: 0,
        board_height_um: 0,
        fill_count: 0,
        hole_count: 0,
        filled_layer_ids: Vec::new(),
        solder_mask_opening_layer_ids: Vec::new(),
        layer_x_extents: Vec::new(),
    };
    if validation.table_count != NATIVE_TABLE_COUNT {
        validation.errors.push(format!(
            "native schema has {} tables, expected {NATIVE_TABLE_COUNT}",
            validation.table_count
        ));
    }
    if validation.index_count != NATIVE_INDEX_COUNT {
        validation.errors.push(format!(
            "native schema has {} indexes, expected {NATIVE_INDEX_COUNT}",
            validation.index_count
        ));
    }
    validate_required_schema(&connection, &mut validation)?;
    if !validation.errors.is_empty() {
        return finish_validation(validation);
    }
    for (table, expected) in [
        ("projects", 1_i64),
        ("project_members", 1),
        ("branches", 2),
        ("project_structures", 1),
        ("history_data", 1),
        ("users", 1),
        ("db_versions", 1),
    ] {
        let count =
            connection.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |row| {
                row.get::<_, i64>(0)
            })?;
        if count != expected {
            validation.errors.push(format!(
                "native table {table} has {count} rows, expected {expected}"
            ));
        }
    }
    if !validation.errors.is_empty() {
        return finish_validation(validation);
    }

    let version = connection
        .query_row(
            "SELECT value FROM db_versions WHERE key = 'sqlite'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if version.as_deref() != Some(SQLITE_SCHEMA_VERSION) {
        validation.errors.push(format!(
            "db_versions sqlite value is {:?}, expected {SQLITE_SCHEMA_VERSION}",
            version
        ));
    }

    let project = connection
        .query_row(
            "SELECT uuid, branch_uuid, owner_uuid FROM projects",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((project_uuid, branch_uuid, owner_uuid)) = project else {
        validation.errors.push("projects has no row".to_owned());
        return finish_validation(validation);
    };
    validation.project_uuid = Some(project_uuid.clone());
    validation.branch_uuid = Some(branch_uuid.clone());

    let expected_project_uuid = sha256_hex(
        absolute_output_path(identity_path)?
            .to_string_lossy()
            .as_bytes(),
    );
    if project_uuid != expected_project_uuid {
        validation.errors.push(format!(
            "project UUID {project_uuid} does not match SHA-256 of its absolute path"
        ));
    }
    if !is_hex_identifier(&branch_uuid) {
        validation
            .errors
            .push("main branch UUID is not 32 lowercase hex characters".to_owned());
        return finish_validation(validation);
    }

    let main_branch = connection
        .query_row(
            "SELECT history_uuid, parent_uuid, node, project_uuid \
             FROM branches WHERE uuid = ?1",
            [&branch_uuid],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((history_uuid, start_uuid, main_node, branch_project_uuid)) = main_branch else {
        validation
            .errors
            .push("projects.branch_uuid does not resolve to a branch".to_owned());
        return finish_validation(validation);
    };
    validation.history_uuid = Some(history_uuid.clone());
    if main_node != 0 || branch_project_uuid != project_uuid {
        validation
            .errors
            .push("main branch node/project linkage is invalid".to_owned());
    }
    let start_valid = connection.query_row(
        "SELECT count(*) FROM branches \
             WHERE uuid = ?1 AND project_uuid = ?2 AND name = 'start' \
             AND node = 1 AND parent_uuid IS NULL",
        params![start_uuid, project_uuid],
        |row| row.get::<_, i64>(0),
    )? == 1;
    if !start_valid {
        validation
            .errors
            .push("start branch linkage is invalid".to_owned());
    }
    if !is_hex_identifier(&history_uuid) {
        validation
            .errors
            .push("history UUID is not 32 lowercase hex characters".to_owned());
        return finish_validation(validation);
    }

    let dynamic_table = format!("project_history_{branch_uuid}");
    let dynamic_exists = connection.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [&dynamic_table],
        |row| row.get::<_, i64>(0),
    )? == 1;
    if !dynamic_exists {
        validation
            .errors
            .push(format!("dynamic history table {dynamic_table} is missing"));
        return finish_validation(validation);
    }
    let dynamic_sql =
        format!("SELECT key, num, parent, snapshot FROM \"{dynamic_table}\" WHERE uuid = ?1");
    let history = connection
        .query_row(&dynamic_sql, [&history_uuid], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .optional()?;
    let Some((key_hex, history_num, parent_history, snapshot)) = history else {
        validation
            .errors
            .push("dynamic project history row is missing".to_owned());
        return finish_validation(validation);
    };
    if history_num != 0 || parent_history.is_some() || snapshot.is_some() {
        validation
            .errors
            .push("initial project history row is not minimal".to_owned());
    }
    let Some(history_key) = decode_fixed_hex::<16>(&key_hex) else {
        validation
            .errors
            .push("history key is not 16-byte hex".to_owned());
        return finish_validation(validation);
    };

    let stored_history = connection
        .query_row(
            "SELECT history_uuid, dataStr FROM history_data WHERE uuid = ?1",
            [&history_uuid],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((stored_history_uuid, encoded_history)) = stored_history else {
        validation
            .errors
            .push("history_data row is missing".to_owned());
        return finish_validation(validation);
    };
    if stored_history_uuid != history_uuid {
        validation
            .errors
            .push("history_data.history_uuid does not match branch history".to_owned());
    }

    let encrypted = match base64_decode(&encoded_history) {
        Ok(value) => value,
        Err(error) => {
            validation
                .errors
                .push(format!("history base64 decode failed: {error}"));
            return finish_validation(validation);
        }
    };
    let payload = match decrypt_history(&encrypted, &history_uuid, &history_key) {
        Ok(value) => value,
        Err(()) => {
            validation
                .errors
                .push("history decrypt/authentication failed".to_owned());
            return finish_validation(validation);
        }
    };
    let Some((edit_head, epru)) = payload.split_once('\n') else {
        validation
            .errors
            .push("history payload has no EDIT_HEAD separator".to_owned());
        return finish_validation(validation);
    };
    let (edit_record, edit_data) = match parse_record(edit_head) {
        Ok(record) => record,
        Err(error) => {
            validation.errors.push(error.to_string());
            return finish_validation(validation);
        }
    };
    if edit_record["type"] != "EDIT_HEAD" || edit_data["uuid"] != owner_uuid {
        validation
            .errors
            .push("history EDIT_HEAD owner does not match projects.owner_uuid".to_owned());
    }

    let documents = match parse_documents(epru) {
        Ok(value) => value,
        Err(error) => {
            validation.errors.push(error.to_string());
            return finish_validation(validation);
        }
    };
    validation.board_uuids = documents.boards.keys().cloned().collect();
    validation.pcb_uuids = documents.pcbs.keys().cloned().collect();
    validation.payload_records = documents.record_count + 1;
    inspect_static_artwork(epru, &documents, &mut validation)?;

    let structure = connection
        .query_row(
            "SELECT structure FROM project_structures \
             WHERE project_uuid = ?1 AND branch_uuid = ?2 ORDER BY id DESC LIMIT 1",
            params![project_uuid, branch_uuid],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match structure {
        Some(structure) => {
            let structure: Value = serde_json::from_str(&structure)?;
            validate_structure(&structure, &documents, &owner_uuid, &mut validation);
        }
        None => validation
            .errors
            .push("project_structures row is missing".to_owned()),
    }
    finish_validation(validation)
}

fn read_public_archive(path: &Path) -> Result<PublicArchive, EasyedaNativeError> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;
    let mut has_metadata = false;
    let mut epru_names = Vec::new();
    for index in 0..archive.len() {
        let name = archive.by_index(index)?.name().to_owned();
        if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
            return Err(EasyedaNativeError::UnsafeArchiveEntry(name));
        }
        if name == "project2.json" {
            has_metadata = true;
        }
        if name.ends_with(".epru") {
            epru_names.push(name);
        }
    }
    if !has_metadata {
        return Err(EasyedaNativeError::MissingProjectMetadata);
    }
    if epru_names.len() != 1 {
        return Err(EasyedaNativeError::MissingEpru);
    }
    let metadata: Value = {
        let mut contents = String::new();
        archive
            .by_name("project2.json")?
            .read_to_string(&mut contents)?;
        serde_json::from_str(&contents)?
    };
    let title = metadata["title"]
        .as_str()
        .filter(|title| !title.trim().is_empty())
        .ok_or(EasyedaNativeError::MissingTitle)?
        .to_owned();
    let mut epru = String::new();
    archive.by_name(&epru_names[0])?.read_to_string(&mut epru)?;
    Ok(PublicArchive { title, epru })
}

/// Recover the public static-artwork semantics from the encrypted native
/// history payload. This is deliberately validation-only: geometry remains
/// authored by the resolved-board exporter, never reconstructed here.
fn inspect_static_artwork(
    epru: &str,
    documents: &ParsedDocuments,
    validation: &mut EasyedaNativeValidation,
) -> Result<(), EasyedaNativeError> {
    let Some(board) = documents.boards.values().next() else {
        return Ok(());
    };
    validation.board_width_um = board.metadata["widthUm"].as_u64().unwrap_or_default() as u32;
    validation.board_height_um = board.metadata["heightUm"].as_u64().unwrap_or_default() as u32;
    if validation.board_width_um == 0 || validation.board_height_um == 0 {
        validation
            .errors
            .push("native BOARD has invalid physical dimensions".to_owned());
    }
    for line in epru.lines().filter(|line| !line.trim().is_empty()) {
        let (head, data) = parse_record(line)?;
        match head["type"].as_str() {
            Some("FILL") => {
                validation.fill_count += 1;
                let Some(layer_id) = data["layerId"].as_u64().map(|value| value as u32) else {
                    validation
                        .errors
                        .push("native FILL has no layerId".to_owned());
                    continue;
                };
                if !validation.filled_layer_ids.contains(&layer_id) {
                    validation.filled_layer_ids.push(layer_id);
                }
            }
            Some("PAD") => validation.hole_count += 1,
            _ => {}
        }
    }
    validation.filled_layer_ids.sort_unstable();
    validation.solder_mask_opening_layer_ids = validation
        .filled_layer_ids
        .iter()
        .copied()
        .filter(|layer_id| matches!(layer_id, 5 | 6))
        .collect();
    validation.layer_x_extents = x_extents_from_static_fills(epru)?;
    Ok(())
}

fn x_extents_from_static_fills(
    epru: &str,
) -> Result<Vec<EasyedaNativeLayerExtent>, EasyedaNativeError> {
    let mut bounds = BTreeMap::<u32, (i64, i64)>::new();
    for line in epru.lines().filter(|line| !line.trim().is_empty()) {
        let (head, data) = parse_record(line)?;
        if head["type"] != "FILL" {
            continue;
        }
        let Some(layer_id) = data["layerId"].as_u64().map(|value| value as u32) else {
            continue;
        };
        for ring in data["path"].as_array().into_iter().flatten() {
            let Some(ring) = ring.as_array() else {
                continue;
            };
            let mut x_values = Vec::new();
            if let Some(x) = ring.first().and_then(Value::as_f64) {
                x_values.push(x);
            }
            x_values.extend(
                ring.get(3..)
                    .into_iter()
                    .flatten()
                    .step_by(2)
                    .filter_map(Value::as_f64),
            );
            for x in x_values {
                let x = (x * 1_000_000.0).round() as i64;
                bounds
                    .entry(layer_id)
                    .and_modify(|(minimum, maximum)| {
                        *minimum = (*minimum).min(x);
                        *maximum = (*maximum).max(x);
                    })
                    .or_insert((x, x));
            }
        }
    }
    Ok(bounds
        .into_iter()
        .map(
            |(layer_id, (min_x_nano_mil, max_x_nano_mil))| EasyedaNativeLayerExtent {
                layer_id,
                min_x_nano_mil,
                max_x_nano_mil,
            },
        )
        .collect())
}

fn parse_documents(epru: &str) -> Result<ParsedDocuments, EasyedaNativeError> {
    let mut all_documents = BTreeMap::<String, Document>::new();
    let mut current_uuid = None;
    let mut record_count = 0;
    for line in epru.lines().filter(|line| !line.trim().is_empty()) {
        record_count += 1;
        let (head, data) = parse_record(line)?;
        match head["type"].as_str() {
            Some("DOCHEAD") => {
                let uuid = data["uuid"]
                    .as_str()
                    .ok_or_else(|| {
                        EasyedaNativeError::MalformedRecord("DOCHEAD has no string uuid".to_owned())
                    })?
                    .to_owned();
                let doc_type = data["docType"]
                    .as_str()
                    .ok_or_else(|| {
                        EasyedaNativeError::MalformedRecord(
                            "DOCHEAD has no string docType".to_owned(),
                        )
                    })?
                    .to_owned();
                let update_time = data["updateTime"].as_u64().unwrap_or_default();
                let version = data["version"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| update_time.to_string());
                all_documents.insert(
                    uuid.clone(),
                    Document {
                        uuid: uuid.clone(),
                        doc_type,
                        version,
                        update_time,
                        metadata: Map::new(),
                    },
                );
                current_uuid = Some(uuid);
            }
            Some("META") => {
                if let Some(document) = current_uuid
                    .as_ref()
                    .and_then(|uuid| all_documents.get_mut(uuid))
                    && let Some(metadata) = data.as_object()
                {
                    document.metadata = metadata.clone();
                }
            }
            _ => {}
        }
    }
    let boards = all_documents
        .iter()
        .filter(|(_, document)| document.doc_type == "BOARD")
        .map(|(uuid, document)| (uuid.clone(), document.clone()))
        .collect();
    let pcbs = all_documents
        .iter()
        .filter(|(_, document)| document.doc_type == "PCB")
        .map(|(uuid, document)| (uuid.clone(), document.clone()))
        .collect();
    Ok(ParsedDocuments {
        boards,
        pcbs,
        record_count,
    })
}

fn parse_record(line: &str) -> Result<(Value, Value), EasyedaNativeError> {
    let line = line.strip_suffix('|').unwrap_or(line);
    let (head, data) = line
        .split_once("||")
        .ok_or_else(|| EasyedaNativeError::MalformedRecord("missing || separator".to_owned()))?;
    Ok((serde_json::from_str(head)?, serde_json::from_str(data)?))
}

fn build_project_structure(documents: &ParsedDocuments, owner_uuid: &str) -> Value {
    let boards = documents
        .boards
        .iter()
        .map(|(uuid, document)| {
            (
                uuid.clone(),
                json!({
                    "uuid": document.uuid,
                    "title": document.metadata.get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("Board1"),
                    "zIndex": document.metadata.get("zIndex").cloned().unwrap_or(Value::Null)
                }),
            )
        })
        .collect::<Map<_, _>>();
    let pcbs = documents
        .pcbs
        .iter()
        .map(|(uuid, document)| {
            let metadata = &document.metadata;
            (
                uuid.clone(),
                json!({
                    "uuid": document.uuid,
                    "title": metadata.get("title").and_then(Value::as_str).unwrap_or("PCB1"),
                    "board": metadata.get("board").and_then(Value::as_str).unwrap_or(""),
                    "zIndex": metadata.get("zIndex").cloned().unwrap_or(Value::Null),
                    "parent_uuid": metadata.get("parent_uuid")
                        .or_else(|| metadata.get("parent"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    "source": metadata.get("source").and_then(Value::as_str).unwrap_or(""),
                    "version": document.version,
                    "updateTime": document.update_time
                }),
            )
        })
        .collect::<Map<_, _>>();
    json!({
        "boards": boards,
        "schematics": {},
        "sheets": {},
        "pcbs": pcbs,
        "panels": {},
        "blockSymbols": {},
        "owner": {
            "uuid": owner_uuid,
            "username": "pcb_atelier",
            "nickname": "PCB Atelier",
            "avatar": ""
        }
    })
}

fn build_history_payload(epru: &str, owner_uuid: &str) -> Result<String, EasyedaNativeError> {
    let update_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let edit_head = format!(
        "{}||{}|",
        serde_json::to_string(&json!({"type": "EDIT_HEAD"}))?,
        serde_json::to_string(&json!({
            "uuid": owner_uuid,
            "username": "",
            "nickname": "",
            "updateTime": update_time
        }))?
    );
    Ok(format!("{edit_head}\n{epru}"))
}

fn write_native_database(
    path: &Path,
    title: &str,
    project_uuid: &str,
    ids: &NativeIds,
    structure: &Value,
    encoded_payload: &str,
) -> Result<(), EasyedaNativeError> {
    let mut connection = Connection::open(path)?;
    connection.execute_batch(BASE_SCHEMA)?;
    let history_table = format!("project_history_{}", ids.main_branch);
    connection.execute_batch(&format!(
        "CREATE TABLE \"{history_table}\" (
            id integer NOT NULL PRIMARY KEY,
            uuid varchar NOT NULL UNIQUE,
            parent varchar NULL,
            snapshot varchar NULL,
            key varchar NOT NULL,
            is_lock integer NOT NULL DEFAULT 0,
            num integer NOT NULL DEFAULT 0,
            created_at datetime NOT NULL DEFAULT (datetime('now')),
            updated_at datetime NOT NULL DEFAULT (datetime('now')),
            lock_time datetime NOT NULL DEFAULT (datetime('1970-01-01 08:00:00')),
            snapshot_num integer NOT NULL DEFAULT 0
        );"
    ))?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO db_versions (key, value) VALUES ('sqlite', ?1)",
        [SQLITE_SCHEMA_VERSION],
    )?;
    transaction.execute(
        "INSERT INTO users
         (uuid, username, nickname, password, preference, avatar, team)
         VALUES (?1, 'pcb_atelier', 'PCB Atelier', '', NULL, '', 0)",
        [&ids.owner],
    )?;
    transaction.execute(
        "INSERT INTO projects
         (uuid, archive, name, content, cbb_project, thumb, ticket, g_ticket,
          owner_uuid, creator_uuid, modifier_uuid, boards,
          block_symbol_attrs_groups, pcb_count, branch_uuid, default_sheet)
         VALUES (?1, 0, ?2, '', 0, '', 1, 1, ?3, ?3, ?3, '{}', '{}', 0, ?4, '')",
        params![project_uuid, title, ids.owner, ids.main_branch],
    )?;
    transaction.execute(
        "INSERT INTO project_members (role, project_uuid, user_uuid)
         VALUES (1, ?1, ?2)",
        params![project_uuid, ids.owner],
    )?;
    transaction.execute(
        "INSERT INTO branches
         (uuid, project_uuid, name, history_uuid, creator_uuid, description,
          parent_uuid, modifier_uuid, node, delete_status)
         VALUES (?1, ?2, 'start', NULL, ?3, '', NULL, ?3, 1, 0)",
        params![ids.start_branch, project_uuid, ids.owner],
    )?;
    transaction.execute(
        "INSERT INTO branches
         (uuid, project_uuid, name, history_uuid, creator_uuid, description,
          parent_uuid, modifier_uuid, node, delete_status)
         VALUES (?1, ?2, 'main', ?3, ?4, '', ?5, ?4, 0, 0)",
        params![
            ids.main_branch,
            project_uuid,
            ids.history,
            ids.owner,
            ids.start_branch
        ],
    )?;
    transaction.execute(
        "INSERT INTO project_structures
         (ticket, project_uuid, branch_uuid, structure)
         VALUES (1, ?1, ?2, ?3)",
        params![
            project_uuid,
            ids.main_branch,
            serde_json::to_string(structure)?
        ],
    )?;
    transaction.execute(
        &format!(
            "INSERT INTO \"{history_table}\"
             (uuid, parent, snapshot, key, is_lock, num, snapshot_num)
             VALUES (?1, NULL, NULL, ?2, 0, 0, 0)"
        ),
        params![ids.history, hex_encode(&ids.history_key)],
    )?;
    transaction.execute(
        "INSERT INTO history_data (uuid, history_uuid, dataStr)
         VALUES (?1, ?1, ?2)",
        params![ids.history, encoded_payload],
    )?;
    transaction.commit()?;
    connection.execute_batch("PRAGMA journal_mode = DELETE;")?;
    Ok(())
}

fn encrypt_history(
    payload: &str,
    history_uuid: &str,
    key: &[u8; 16],
) -> Result<Vec<u8>, EasyedaNativeError> {
    let Some(iv) = decode_fixed_hex::<16>(history_uuid) else {
        return Err(EasyedaNativeError::Encryption);
    };
    let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
    gzip.write_all(payload.as_bytes())?;
    let compressed = gzip.finish()?;
    Aes128GcmWith16ByteNonce::new_from_slice(key)
        .map_err(|_| EasyedaNativeError::Encryption)?
        .encrypt(Nonce::<U16>::from_slice(&iv), compressed.as_ref())
        .map_err(|_| EasyedaNativeError::Encryption)
}

fn decrypt_history(encrypted: &[u8], history_uuid: &str, key: &[u8; 16]) -> Result<String, ()> {
    let iv = decode_fixed_hex::<16>(history_uuid).ok_or(())?;
    let compressed = Aes128GcmWith16ByteNonce::new_from_slice(key)
        .map_err(|_| ())?
        .decrypt(Nonce::<U16>::from_slice(&iv), encrypted)
        .map_err(|_| ())?;
    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut payload = String::new();
    decoder.read_to_string(&mut payload).map_err(|_| ())?;
    Ok(payload)
}

fn validate_required_schema(
    connection: &Connection,
    validation: &mut EasyedaNativeValidation,
) -> Result<(), EasyedaNativeError> {
    for name in [
        "projects",
        "project_members",
        "branches",
        "project_structures",
        "history_data",
        "documents",
        "schematics",
        "components",
        "devices",
        "resources",
        "coppers",
        "texts",
        "users",
        "db_versions",
    ] {
        let exists = connection.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get::<_, i64>(0),
        )? == 1;
        if !exists {
            validation
                .errors
                .push(format!("native schema is missing table {name}"));
        }
    }
    Ok(())
}

fn validate_structure(
    structure: &Value,
    documents: &ParsedDocuments,
    owner_uuid: &str,
    validation: &mut EasyedaNativeValidation,
) {
    if structure["owner"]["uuid"] != owner_uuid {
        validation
            .errors
            .push("project structure owner does not match projects.owner_uuid".to_owned());
    }
    if structure["boards"].as_object().map(Map::len) != Some(documents.boards.len()) {
        validation
            .errors
            .push("project structure BOARD count does not match payload".to_owned());
    }
    if structure["pcbs"].as_object().map(Map::len) != Some(documents.pcbs.len()) {
        validation
            .errors
            .push("project structure PCB count does not match payload".to_owned());
    }
    for (uuid, document) in &documents.boards {
        if structure["boards"][uuid]["uuid"] != *uuid {
            validation
                .errors
                .push(format!("project structure is missing BOARD {uuid}"));
        }
        let title = document
            .metadata
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Board1");
        if structure["boards"][uuid]["title"] != title {
            validation.errors.push(format!(
                "project structure BOARD {uuid} title differs from payload"
            ));
        }
        let z_index = document
            .metadata
            .get("zIndex")
            .cloned()
            .unwrap_or(Value::Null);
        if structure["boards"][uuid]["zIndex"] != z_index {
            validation.errors.push(format!(
                "project structure BOARD {uuid} zIndex differs from payload"
            ));
        }
    }
    for (uuid, document) in &documents.pcbs {
        if structure["pcbs"][uuid]["uuid"] != *uuid {
            validation
                .errors
                .push(format!("project structure is missing PCB {uuid}"));
        }
        let payload_board = document
            .metadata
            .get("board")
            .and_then(Value::as_str)
            .unwrap_or("");
        if structure["pcbs"][uuid]["board"] != payload_board {
            validation.errors.push(format!(
                "project structure PCB {uuid} board linkage does not match payload"
            ));
        }
        let title = document
            .metadata
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("PCB1");
        if structure["pcbs"][uuid]["title"] != title
            || structure["pcbs"][uuid]["version"] != document.version
            || structure["pcbs"][uuid]["updateTime"] != document.update_time
        {
            validation.errors.push(format!(
                "project structure PCB {uuid} metadata differs from payload"
            ));
        }
        let parent_uuid = document
            .metadata
            .get("parent_uuid")
            .or_else(|| document.metadata.get("parent"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let source = document
            .metadata
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("");
        if structure["pcbs"][uuid]["parent_uuid"] != parent_uuid
            || structure["pcbs"][uuid]["source"] != source
        {
            validation.errors.push(format!(
                "project structure PCB {uuid} parent/source differs from payload"
            ));
        }
        if !documents.boards.contains_key(payload_board) {
            validation.errors.push(format!(
                "PCB {uuid} refers to unknown BOARD {payload_board}"
            ));
        }
    }
}

fn finish_validation(
    mut validation: EasyedaNativeValidation,
) -> Result<EasyedaNativeValidation, EasyedaNativeError> {
    validation.is_valid = validation.errors.is_empty();
    Ok(validation)
}

fn schema_count(connection: &Connection, kind: &str) -> Result<i64, rusqlite::Error> {
    connection.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = ?1",
        [kind],
        |row| row.get(0),
    )
}

fn absolute_output_path(path: &Path) -> Result<PathBuf, EasyedaNativeError> {
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let parent = path.parent().ok_or(EasyedaNativeError::MissingParent)?;
    let file_name = path.file_name().ok_or(EasyedaNativeError::MissingParent)?;
    let absolute_parent = if parent.as_os_str().is_empty() {
        std::env::current_dir()?
    } else {
        parent.canonicalize()?
    };
    Ok(absolute_parent.join(file_name))
}

fn random_uuid() -> String {
    hex_encode(&random_bytes::<16>())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0_u8; N];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

fn sha256_hex(input: &[u8]) -> String {
    hex_encode(&Sha256::digest(input))
}

fn hex_encode(input: &[u8]) -> String {
    input.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_fixed_hex<const N: usize>(input: &str) -> Option<[u8; N]> {
    if input.len() != N * 2 {
        return None;
    }
    let mut output = [0_u8; N];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&input[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(output)
}

fn is_hex_identifier(input: &str) -> bool {
    input.len() == 32
        && input
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or_default();
        let c = chunk.get(2).copied().unwrap_or_default();
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return Err("invalid encoded length");
    }
    let mut output = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        let a = value(chunk[0]).ok_or("invalid alphabet")?;
        let b = value(chunk[1]).ok_or("invalid alphabet")?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            value(chunk[2]).ok_or("invalid alphabet")?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            value(chunk[3]).ok_or("invalid alphabet")?
        };
        if (chunk[2] == b'=' || chunk[3] == b'=') && index + 1 != bytes.len() / 4 {
            return Err("padding before final quartet");
        }
        output.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            output.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            output.push((c << 6) | d);
        }
    }
    Ok(output)
}

const BASE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS "attributes" (
    "key" text NOT NULL,
    "value" text NOT NULL,
    "device_uuid" varchar NOT NULL,
    PRIMARY KEY (device_uuid, key)
);
CREATE TABLE IF NOT EXISTS "backups" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "limit" integer,
    "project_uuid" varchar NOT NULL,
    "auto" boolean NOT NULL DEFAULT (0),
    "name" varchar NOT NULL,
    "description" varchar,
    "archive" boolean NOT NULL DEFAULT (0),
    "user_uuid" varchar NOT NULL,
    "owner_uuid" varchar NOT NULL,
    "project_name" varchar NOT NULL,
    "createtime" datetime NOT NULL DEFAULT (datetime('now')),
    "path" varchar NOT NULL DEFAULT ('')
);
CREATE TABLE IF NOT EXISTS "block_symbol_attributes" (
    "path" varchar PRIMARY KEY NOT NULL,
    "project_uuid" varchar NOT NULL,
    "hash" integer NOT NULL,
    "ticket" integer NOT NULL DEFAULT (1),
    "attr" text NOT NULL
);
CREATE TABLE IF NOT EXISTS "boards" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "project_uuid" varchar (32) NOT NULL,
    "sch_uuid" varchar (32) NOT NULL,
    "name" varchar (255) NOT NULL,
    "sort" INTEGER NOT NULL,
    CONSTRAINT "project" FOREIGN KEY ("project_uuid") REFERENCES "projects" ("uuid")
);
CREATE TABLE IF NOT EXISTS "categories" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "name" varchar NOT NULL,
    "type" integer NOT NULL,
    "user_uuid" varchar,
    "parent_uuid" varchar,
    "name_cn" varchar,
    "order_no" REAL,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS "coppers" (
    "path" varchar NOT NULL,
    "project_uuid" varchar NOT NULL,
    "dataStr" varchar NOT NULL,
    "ticket" integer NOT NULL DEFAULT (1),
    PRIMARY KEY ("path", "project_uuid")
);
CREATE TABLE IF NOT EXISTS "notifications" (
    "uuid" integer PRIMARY KEY AUTOINCREMENT NOT NULL,
    "user_uuid" varchar NOT NULL,
    "content" varchar NOT NULL,
    "type" varchar NOT NULL,
    "status" varchar NOT NULL,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS "project_logs" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "project_uuid" varchar NOT NULL,
    "content" varchar NOT NULL,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS "project_members" (
    "role" integer NOT NULL,
    "project_uuid" varchar NOT NULL,
    "user_uuid" varchar NOT NULL,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY ("project_uuid", "user_uuid")
);
CREATE TABLE IF NOT EXISTS "schematics" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "description" varchar NOT NULL DEFAULT (''),
    "ticket" integer NOT NULL DEFAULT (1),
    "sheet_count" integer NOT NULL DEFAULT (0),
    "project_uuid" varchar NOT NULL,
    "name" varchar NOT NULL,
    "display_name" varchar NOT NULL,
    "createtime" integer NOT NULL,
    "updatetime" integer NOT NULL,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now')),
    "sort" varchar NOT NULL DEFAULT ('')
);
CREATE TABLE IF NOT EXISTS "sessions" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "key" varchar NOT NULL,
    "value" text NOT NULL
);
CREATE TABLE IF NOT EXISTS "system_attributes" (
    "property" varchar (255) PRIMARY KEY NOT NULL,
    "type" varchar (255) NOT NULL,
    "object" varchar (255) NOT NULL,
    "show_status" varchar
);
CREATE TABLE IF NOT EXISTS "team_members" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "role" integer NOT NULL,
    "team_uuid" varchar,
    "user_uuid" varchar,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS "users" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "username" varchar NOT NULL,
    "nickname" varchar NOT NULL,
    "password" varchar (255),
    "preference" varchar,
    "avatar" varchar,
    "team" boolean NOT NULL DEFAULT (0),
    CONSTRAINT "UQ_4baf95322bd69fe419c26c5430c" UNIQUE ("username")
);
CREATE UNIQUE INDEX "IDX_357dd5c8dce48c981717076e42" ON "backups" ("uuid" ASC);
CREATE UNIQUE INDEX "IDX_018134254db44b1d2c7cafd642" ON "project_logs" ("uuid" ASC);
CREATE UNIQUE INDEX "IDX_3144ec03a84b838fc172774812" ON "schematics" ("uuid" ASC);
CREATE UNIQUE INDEX "IDX_b22d7f930655e00269912e818c" ON "schematics" (
    "project_uuid" ASC,
    "name" ASC
);
CREATE UNIQUE INDEX "IDX_46e9305a61d8ef1c08668ca606" ON "team_members" ("uuid" ASC);
CREATE UNIQUE INDEX "IDX_951b8f1dfc94ac1d0301a14b7e" ON "users" ("uuid" ASC);
CREATE TABLE IF NOT EXISTS "system_config" (
    "key" varchar PRIMARY KEY NOT NULL,
    "value" varchar NOT NULL
);
CREATE TABLE IF NOT EXISTS "db_paths" (
    "path" varchar NOT NULL PRIMARY KEY,
    "name" varchar NOT NULL,
    "version" varchar NOT NULL,
    "system" boolean NOT NULL,
    "type" INTEGER NOT NULL,
    "last_open_time" datetime NOT NULL
);
CREATE INDEX "attributes_device_uuid" ON "attributes" ("device_uuid" ASC);
CREATE INDEX "attributes_key" ON "attributes" ("key" ASC);
CREATE INDEX "attributes_value" ON "attributes" ("value" ASC);
CREATE INDEX "categories_uuid" ON "categories" ("uuid" ASC);
CREATE TABLE IF NOT EXISTS "db_versions" (
    "key" varchar PRIMARY KEY NOT NULL,
    "value" varchar NOT NULL
);
CREATE TABLE IF NOT EXISTS "resources" (
    "hash" varchar PRIMARY KEY NOT NULL,
    "dataStr" varchar NOT NULL,
    "filename" varchar,
    "owner_uuid" varchar,
    "ticket" integer NOT NULL DEFAULT (1)
);
CREATE INDEX "resources_hash" ON "resources" ("hash" ASC);
CREATE INDEX "resources_owner_uuid" ON "resources" ("owner_uuid" ASC);
CREATE TABLE IF NOT EXISTS "texts" (
    "path" varchar NOT NULL,
    "project_uuid" varchar NOT NULL,
    "dataStr" varchar NOT NULL,
    "ticket" integer NOT NULL DEFAULT (1),
    PRIMARY KEY ("path", "project_uuid")
);
CREATE TABLE IF NOT EXISTS "editor_caches" (
    "key" varchar NOT NULL,
    "value" TEXT NOT NULL,
    PRIMARY KEY ("key")
);
CREATE TABLE IF NOT EXISTS "broadcast_messages" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "key" varchar NOT NULL,
    "value" varchar NOT NULL,
    "created_at" REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS "components_tmp" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "title" varchar NOT NULL,
    "display_title" varchar NOT NULL,
    "description" varchar NOT NULL,
    "source" varchar,
    "version" varchar,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now')),
    "ticket" integer NOT NULL,
    "docType" integer NOT NULL,
    "dataStr" text NOT NULL,
    "createTime" datetime NOT NULL DEFAULT (datetime('now')),
    "updateTime" datetime NOT NULL DEFAULT (datetime('now')),
    "modifier_uuid" varchar,
    "creator_uuid" varchar,
    "owner_uuid" varchar,
    "project_uuid" varchar,
    "child_tag" varchar NOT NULL DEFAULT (''),
    "parent_tag" varchar NOT NULL DEFAULT (''),
    "custom_tags" varchar DEFAULT ('')
);
CREATE TABLE IF NOT EXISTS "documents" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "title" varchar NOT NULL,
    "display_title" varchar NOT NULL,
    "description" varchar NOT NULL,
    "docType" integer NOT NULL,
    "dataStr" text NOT NULL,
    "sheet_id" integer NOT NULL DEFAULT (1),
    "ticket" integer NOT NULL DEFAULT (1),
    "sort_ticket" integer NOT NULL DEFAULT (0),
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now')),
    "creator_uuid" varchar,
    "schematic_uuid" varchar,
    "project_uuid" varchar,
    "image" text,
    "parent_uuid" varchar
);
CREATE UNIQUE INDEX "IDX_00271f3c9caae51f3b6a41b37b" ON "documents" (
    "schematic_uuid" ASC,
    "title" ASC,
    "project_uuid" ASC,
    "docType" ASC
);
CREATE UNIQUE INDEX "IDX_f6ab4fff7a383f1f14013ab270" ON "documents" ("uuid" ASC);
CREATE TABLE IF NOT EXISTS "component_histories" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "uuid" varchar NOT NULL,
    "parent" varchar NULL,
    "snapshot" varchar NULL,
    "key" varchar NOT NULL,
    "iv" varchar NOT NULL,
    "num" integer NOT NULL DEFAULT (0),
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS "branches" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "uuid" varchar NOT NULL,
    "project_uuid" varchar NOT NULL,
    "name" varchar NOT NULL,
    "history_uuid" varchar NULL,
    "creator_uuid" varchar NOT NULL,
    "description" varchar NOT NULL,
    "parent_uuid" varchar NULL,
    "modifier_uuid" varchar NOT NULL,
    "node" integer NOT NULL DEFAULT (0),
    "delete_status" integer NOT NULL DEFAULT (0),
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS "branch_locks" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "document_uuid" varchar NOT NULL,
    "user_uuid" varchar NOT NULL,
    "branch_uuid" varchar NOT NULL,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS "project_images" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "uuid" varchar NOT NULL,
    "ticket" integer NOT NULL DEFAULT (0),
    "project_uuid" varchar NOT NULL,
    "branch_uuid" varchar NOT NULL,
    "url" varchar NOT NULL,
    "image_data" varchar NULL
);
CREATE TABLE IF NOT EXISTS "project_structures" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "ticket" integer NOT NULL DEFAULT (0),
    "project_uuid" varchar NOT NULL,
    "branch_uuid" varchar NOT NULL,
    "structure" TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS "project_histories" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "uuid" varchar NOT NULL,
    "parent" varchar NULL,
    "snapshot" varchar NULL,
    "key" varchar NOT NULL,
    "is_lock" integer NOT NULL DEFAULT (0),
    "num" integer NOT NULL DEFAULT (0),
    "snapshot_num" integer NOT NULL DEFAULT (0),
    "lock_time" datetime NOT NULL DEFAULT ('1970-01-01 08:00:00'),
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS "history_data" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "uuid" varchar UNIQUE NOT NULL,
    "history_uuid" varchar NOT NULL,
    "dataStr" TEXT NOT NULL,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS "components" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "title" varchar NOT NULL,
    "display_title" varchar NOT NULL,
    "description" varchar NOT NULL,
    "source" varchar,
    "version" varchar,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now')),
    "ticket" integer NOT NULL,
    "docType" integer NOT NULL,
    "dataStr" text NOT NULL,
    "createTime" datetime NOT NULL DEFAULT (datetime('now')),
    "updateTime" datetime NOT NULL DEFAULT (datetime('now')),
    "modifier_uuid" varchar,
    "creator_uuid" varchar,
    "owner_uuid" varchar,
    "project_uuid" varchar,
    "child_tag" varchar NOT NULL DEFAULT (''),
    "parent_tag" varchar NOT NULL DEFAULT (''),
    "custom_tags" varchar DEFAULT (''),
    "history_uuid" varchar
);
CREATE INDEX "IDX_f9312828d80136f7afaf47c554" ON "components" ("project_uuid" ASC, "title" ASC, "docType" ASC);
CREATE UNIQUE INDEX "IDX_fba3398cf283439c13afec000e" ON "components" ("uuid" ASC);
CREATE INDEX "components_updateTime" ON "components" ("updateTime" DESC);
CREATE INDEX "components_docType" ON "components" ("docType" ASC);
CREATE INDEX "components_project_uuid" ON "components" ("project_uuid" ASC);
CREATE TABLE IF NOT EXISTS "devices" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "description" varchar NOT NULL,
    "title" varchar NOT NULL,
    "display_title" varchar NOT NULL,
    "images" text NOT NULL DEFAULT (''),
    "source" varchar,
    "version" varchar,
    "ticket" integer NOT NULL,
    "footprint_type" integer,
    "symbol_type" integer,
    "created_at" datetime NOT NULL DEFAULT (datetime('now')),
    "updated_at" datetime NOT NULL DEFAULT (datetime('now')),
    "createTime" datetime NOT NULL DEFAULT (datetime('now')),
    "updateTime" datetime NOT NULL DEFAULT (datetime('now')),
    "modifier_uuid" varchar,
    "creator_uuid" varchar,
    "owner_uuid" varchar,
    "project_uuid" varchar NOT NULL,
    "child_tag" varchar NOT NULL DEFAULT (''),
    "parent_tag" varchar NOT NULL DEFAULT (''),
    "custom_tags" varchar DEFAULT (''),
    "history_uuid" varchar
);
CREATE INDEX "devices_title_owner_uuid_project_uuid" ON "devices" ("project_uuid" ASC, "title" ASC, "owner_uuid" ASC);
CREATE UNIQUE INDEX "IDX_707b5b8b374103d40974e670d3" ON "devices" ("uuid" ASC);
CREATE INDEX "devices_updateTime" ON "devices" ("updateTime" DESC);
CREATE TABLE IF NOT EXISTS "projects" (
    "uuid" varchar PRIMARY KEY NOT NULL,
    "archive" boolean NOT NULL,
    "name" varchar NOT NULL,
    "content" varchar NOT NULL,
    "cbb_project" boolean default 0 NOT NULL,
    "thumb" varchar NOT NULL,
    "ticket" integer NOT NULL,
    "g_ticket" integer default 1 NOT NULL,
    "owner_uuid" varchar,
    "creator_uuid" varchar,
    "created_at" datetime default (datetime('now')) NOT NULL,
    "updated_at" datetime default (datetime('now')) NOT NULL,
    "modifier_uuid" varchar,
    "boards" varchar default '{}' NOT NULL,
    "block_symbol_attrs_groups" varchar default '{}' NOT NULL,
    "pcb_count" integer default 0 NOT NULL,
    "branch_uuid" varchar null,
    "default_sheet" text default ''
);
CREATE UNIQUE INDEX "IDX_fc9f1e64d4626f18beff534a9f" ON "projects" ("uuid");
"#;
