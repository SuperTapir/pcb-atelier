use std::io::Write;

use atelier_core::{
    AtelierDocument, PROJECT_FORMAT, PROJECT_SCHEMA_VERSION, ProjectBundle, ProjectError,
};

#[test]
fn project_archive_round_trip_preserves_document_and_embedded_asset() {
    let document = AtelierDocument::new_card("项目包", 64_000, 100_000);
    let mut bundle = ProjectBundle::new(document);
    let image_bytes = b"fake-png-for-contract-test".to_vec();
    let asset_id = bundle
        .embed_asset(
            "character.png",
            "image/png",
            2048,
            3072,
            image_bytes.clone(),
        )
        .expect("embed asset");
    let temp = tempfile::tempdir().expect("temporary project directory");
    let project_path = temp.path().join("artwork.pcba");

    bundle.save(&project_path).expect("save project");
    let restored = ProjectBundle::open(&project_path).expect("open project");

    assert_eq!(restored.document, bundle.document);
    assert_eq!(restored.asset_bytes(asset_id), Some(image_bytes.as_slice()));
    assert_eq!(
        restored.document.assets[0].embedded_path,
        format!("assets/{asset_id}.png")
    );
}

#[test]
fn saving_rejects_missing_or_modified_asset_bytes() {
    let document = AtelierDocument::new_card("缺失资产", 64_000, 100_000);
    let mut bundle = ProjectBundle::new(document);
    let asset_id = bundle
        .embed_asset("source.png", "image/png", 10, 10, b"original".to_vec())
        .expect("embed asset");
    let temp = tempfile::tempdir().expect("temporary project directory");
    let project_path = temp.path().join("artwork.pcba");

    bundle.assets.remove(&asset_id);
    let error = bundle
        .save(&project_path)
        .expect_err("missing bytes must fail");
    assert!(matches!(error, ProjectError::MissingAsset(id) if id == asset_id));

    bundle.assets.insert(asset_id, b"modified".to_vec());
    let error = bundle
        .save(&project_path)
        .expect_err("hash mismatch must fail");
    assert!(matches!(error, ProjectError::AssetHashMismatch(id) if id == asset_id));
}

#[test]
fn opening_rejects_archive_with_unsafe_entry() {
    let temp = tempfile::tempdir().expect("temporary project directory");
    let project_path = temp.path().join("unsafe.pcba");
    let file = std::fs::File::create(&project_path).expect("create archive");
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("../escape.png", zip::write::SimpleFileOptions::default())
        .expect("start unsafe entry");
    archive.write_all(b"escape").expect("write unsafe entry");
    archive.finish().expect("finish archive");

    let error = ProjectBundle::open(&project_path).expect_err("unsafe path must fail");
    assert!(matches!(error, ProjectError::UnsafeArchivePath(_)));
}

#[test]
fn opening_a_v1_project_migrates_it_to_the_current_schema() {
    let temp = tempfile::tempdir().expect("temporary project directory");
    let project_path = temp.path().join("legacy.pcba");
    let mut legacy_document = AtelierDocument::new_card("旧工程", 64_000, 100_000);
    legacy_document.schema_version = 1;
    let manifest = serde_json::json!({
        "format": PROJECT_FORMAT,
        "schemaVersion": 1,
        "document": legacy_document,
    });
    let file = std::fs::File::create(&project_path).expect("create archive");
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .expect("start manifest");
    archive
        .write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
        .expect("write manifest");
    archive.finish().expect("finish archive");

    let migrated = ProjectBundle::open(&project_path).expect("open legacy project");
    assert_eq!(migrated.document.schema_version, PROJECT_SCHEMA_VERSION);
    migrated.document.validate().expect("migrated document");
}
