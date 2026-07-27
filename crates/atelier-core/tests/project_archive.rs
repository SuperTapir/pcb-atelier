use std::io::{Cursor, Write};

use atelier_core::{
    AtelierDocument, CardSide, CombineMode, ContentLayer, FaceProductionLayer, ImageTreatment,
    PROJECT_FORMAT, PROJECT_SCHEMA_VERSION, ProductionMapping, ProductionTarget, ProjectBundle,
    ProjectBundleRasterizer, ProjectError, SamplingPurpose, SolderMaskColor, TransformUm,
    TreatmentCompileRequest, TreatmentRecipe, compile_fabrication_plan, compile_image_treatment,
    resolve_fabrication_plan_for_purpose,
};
use image::{DynamicImage, ImageFormat};

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
fn save_overwrite_and_save_as_reopen_the_same_complete_project() {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("保存闭环", 64_000, 100_000));
    let text = ContentLayer::new_text(
        "背面文字",
        "Kamome",
        TransformUm::rect(22, 22, 20_000, 6_000),
    );
    let text_id = text.id;
    bundle.document.back.layers.push(text);
    bundle.document.mappings.push(ProductionMapping::new(
        text_id,
        ProductionTarget::new(CardSide::Back, FaceProductionLayer::Silkscreen),
        CombineMode::Add,
    ));
    let temp = tempfile::tempdir().expect("temporary project directory");
    let original_path = temp.path().join("original.pcba");
    let save_as_path = temp.path().join("copy.pcba");

    bundle.save(&original_path).expect("first save");
    bundle.document.title = "保存闭环（已修改）".to_owned();
    bundle.save(&original_path).expect("overwrite save");
    let overwritten = ProjectBundle::open(&original_path).expect("open overwritten project");
    assert_eq!(overwritten.document.title, "保存闭环（已修改）");
    assert_eq!(
        overwritten.document.back.layers[0].kind,
        bundle.document.back.layers[0].kind
    );

    bundle.save(&save_as_path).expect("save as");
    let copied = ProjectBundle::open(&save_as_path).expect("open save-as project");
    assert_eq!(copied, bundle);
    assert_eq!(
        ProjectBundle::open(&original_path)
            .expect("original remains valid")
            .document
            .title,
        "保存闭环（已修改）"
    );
}

#[test]
fn archive_round_trip_preserves_asset_treatment_instance_mapping_and_manufacturer() {
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::new_rgba8(4, 3)
        .write_to(&mut encoded, ImageFormat::Png)
        .expect("encode source");
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("完整领域", 64_000, 100_000));
    let asset_id = bundle
        .embed_asset("logo.png", "image/png", 4, 3, encoded.into_inner())
        .expect("embed asset");
    bundle.document.assets[0].folder_path = Some("品牌/Logo".to_owned());
    bundle.document.assets[0].tags = vec!["正面".to_owned()];
    let mut treatment = ImageTreatment::new(asset_id, TreatmentRecipe::standard_monochrome());
    treatment.recipe.invert = true;
    let treatment_id = treatment.id;
    bundle.document.image_treatments.push(treatment);
    let layer = ContentLayer::new_image(
        "共享 Logo",
        asset_id,
        TransformUm::rect(12_000, 18_000, 24_000, 18_000),
    );
    let layer_id = layer.id;
    bundle.document.front.layers.push(layer);
    let mut mapping = ProductionMapping::new(
        layer_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
        CombineMode::Add,
    );
    mapping.treatment_id = Some(treatment_id);
    bundle.document.mappings.push(mapping);
    bundle.document.manufacturer_profile.solder_mask = SolderMaskColor::White;
    bundle.document.stackup.solder_mask_color = SolderMaskColor::White;

    let temp = tempfile::tempdir().expect("temporary project directory");
    let project_path = temp.path().join("complete.pcba");
    bundle.save(&project_path).expect("save complete project");
    let restored = ProjectBundle::open(&project_path).expect("reopen complete project");

    assert_eq!(restored, bundle);
}

#[test]
fn embedded_source_rebuilds_treatment_after_original_and_proxy_cache_are_removed() {
    let temp = tempfile::tempdir().expect("temporary project directory");
    let source_path = temp.path().join("source.png");
    let cache_path = temp.path().join("proxy-cache.bin");
    let project_path = temp.path().join("portable.pcba");
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(3, 2)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .expect("encode source image");
    std::fs::write(&source_path, encoded.into_inner()).expect("write original source");
    std::fs::write(&cache_path, b"discardable proxy").expect("write proxy cache");

    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("可移植工程", 8_000, 12_000));
    let asset_id = bundle
        .embed_asset(
            "source.png",
            "image/png",
            3,
            2,
            std::fs::read(&source_path).expect("read source"),
        )
        .expect("embed source");
    let treatment = ImageTreatment::new(asset_id, TreatmentRecipe::default());
    let treatment_id = treatment.id;
    bundle.document.image_treatments.push(treatment);
    let layer = ContentLayer::new_image(
        "可重建图片",
        asset_id,
        TransformUm::rect(2_000, 3_000, 3_000, 2_000),
    );
    let layer_id = layer.id;
    bundle.document.front.layers.push(layer);
    let mut mapping = ProductionMapping::new(
        layer_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
        CombineMode::Add,
    );
    mapping.treatment_id = Some(treatment_id);
    bundle.document.mappings.push(mapping);
    bundle.save(&project_path).expect("save portable project");

    std::fs::remove_file(&source_path).expect("remove original source");
    std::fs::remove_file(&cache_path).expect("clear proxy cache");
    let restored = ProjectBundle::open(&project_path).expect("reopen from archive");
    let treatment = restored
        .document
        .image_treatments
        .iter()
        .find(|treatment| treatment.id == treatment_id)
        .expect("restored treatment");
    let compiled = compile_image_treatment(
        restored
            .asset_bytes(asset_id)
            .expect("embedded source bytes"),
        &treatment.recipe,
        TreatmentCompileRequest::for_purpose(3_000, 2_000, 7, SamplingPurpose::InteractiveProxy),
    )
    .expect("rebuild proxy without external source or cache");

    assert_eq!(compiled.revision, 7);
    assert_eq!(compiled.recipe_fingerprint, treatment.recipe.fingerprint());
    assert_eq!(compiled.mask.sha256().len(), 64);
    let plan = compile_fabrication_plan(&restored.document).expect("compile formal plan");
    let mut rasterizer =
        ProjectBundleRasterizer::new(&restored).expect("rebuild rasterizer from archive");
    let board = resolve_fabrication_plan_for_purpose(
        &plan,
        SamplingPurpose::FormalProduction,
        &mut rasterizer,
    )
    .expect("rebuild formal production output");
    let preview = board
        .preview_textures()
        .expect("rebuild 3D preview textures");
    assert_eq!(preview.front.width_px, board.grid.width_px);
    assert_eq!(preview.front.height_px, board.grid.height_px);
    assert!(!source_path.exists());
    assert!(!cache_path.exists());
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
fn opening_an_older_unreleased_schema_is_rejected_without_rewriting_it() {
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

    let before = std::fs::read(&project_path).expect("original bytes");
    let error = ProjectBundle::open(&project_path).expect_err("old schema is unsupported");
    assert!(matches!(error, ProjectError::UnsupportedManifestSchema(1)));
    assert_eq!(
        std::fs::read(&project_path).expect("project remains readable"),
        before
    );
}

#[test]
fn opening_accepts_only_the_exact_current_manifest_schema() {
    for schema_version in [PROJECT_SCHEMA_VERSION - 1, PROJECT_SCHEMA_VERSION + 1] {
        let temp = tempfile::tempdir().expect("temporary project directory");
        let project_path = temp.path().join(format!("schema-{schema_version}.pcba"));
        let mut document = AtelierDocument::new_card("非当前工程", 64_000, 100_000);
        document.schema_version = schema_version;
        write_archive(
            &project_path,
            &serde_json::json!({
                "format": PROJECT_FORMAT,
                "schemaVersion": schema_version,
                "document": document,
            }),
            &[],
        );

        let error =
            ProjectBundle::open(&project_path).expect_err("non-current schema must be rejected");
        assert!(
            matches!(error, ProjectError::UnsupportedManifestSchema(version) if version == schema_version)
        );
    }
}

#[test]
fn opening_rejects_unknown_manifest_fields_with_the_field_path() {
    let temp = tempfile::tempdir().expect("temporary project directory");
    let project_path = temp.path().join("unknown-manifest-field.pcba");
    let document = AtelierDocument::new_card("未知字段", 64_000, 100_000);
    write_archive(
        &project_path,
        &serde_json::json!({
            "format": PROJECT_FORMAT,
            "schemaVersion": PROJECT_SCHEMA_VERSION,
            "document": document,
            "legacyCache": { "preview": "stale" },
        }),
        &[],
    );

    let error = ProjectBundle::open(&project_path).expect_err("unknown field must fail");
    assert!(
        matches!(
            error,
            ProjectError::InvalidManifestField { ref path, .. }
                if path.contains("legacyCache")
        ),
        "error should identify the unknown manifest field path: {error}"
    );
}

#[test]
fn opening_rejects_cache_and_legacy_entries_not_referenced_by_the_manifest() {
    for unexpected_path in ["cache/preview.png", "legacy/document.json"] {
        let temp = tempfile::tempdir().expect("temporary project directory");
        let project_path = temp.path().join("unexpected-entry.pcba");
        let document = AtelierDocument::new_card("严格归档", 64_000, 100_000);
        write_archive(
            &project_path,
            &serde_json::json!({
                "format": PROJECT_FORMAT,
                "schemaVersion": PROJECT_SCHEMA_VERSION,
                "document": document,
            }),
            &[(unexpected_path, b"stale")],
        );

        let error =
            ProjectBundle::open(&project_path).expect_err("unexpected archive entry must fail");
        assert!(
            matches!(
                error,
                ProjectError::UnexpectedArchiveEntry(ref path) if path == unexpected_path
            ),
            "unexpected archive error: {error}"
        );
    }
}

#[test]
fn current_schema_rejects_asset_records_missing_current_required_fields() {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("严格素材字段", 64_000, 100_000));
    let bytes = b"strict-current-asset".to_vec();
    let asset_id = bundle
        .embed_asset("strict.png", "image/png", 1, 1, bytes.clone())
        .expect("embed strict asset");
    let mut manifest = serde_json::json!({
        "format": PROJECT_FORMAT,
        "schemaVersion": bundle.document.schema_version,
        "document": bundle.document,
    });
    manifest["document"]["assets"][0]
        .as_object_mut()
        .expect("asset object")
        .remove("folderPath");

    let temp = tempfile::tempdir().expect("temporary project directory");
    let project_path = temp.path().join("missing-current-field.pcba");
    let file = std::fs::File::create(&project_path).expect("create archive");
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .expect("start manifest");
    writer
        .write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
        .expect("write manifest");
    writer
        .start_file(
            format!("assets/{asset_id}.png"),
            zip::write::SimpleFileOptions::default(),
        )
        .expect("start asset");
    writer.write_all(&bytes).expect("write asset");
    writer.finish().expect("finish archive");

    let error = ProjectBundle::open(&project_path).expect_err("missing field must fail");
    assert!(
        error.to_string().contains("folderPath"),
        "error should identify the missing current field: {error}"
    );
}

#[test]
fn invalid_manifest_reports_the_document_field_path() {
    let temp = tempfile::tempdir().expect("temporary project directory");
    let project_path = temp.path().join("invalid-field.pcba");
    let document = AtelierDocument::new_card("字段错误", 64_000, 100_000);
    let mut manifest = serde_json::json!({
        "format": PROJECT_FORMAT,
        "schemaVersion": document.schema_version,
        "document": document,
    });
    manifest["document"]["board"]["width_um"] = serde_json::json!("wide");
    let file = std::fs::File::create(&project_path).expect("create archive");
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .expect("start manifest");
    archive
        .write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
        .expect("write manifest");
    archive.finish().expect("finish archive");

    let error = ProjectBundle::open(&project_path).expect_err("field must fail");
    assert!(
        error.to_string().contains("document.board"),
        "unexpected field error: {error}"
    );
}

#[test]
fn failed_save_keeps_an_existing_project_byte_for_byte() {
    let temp = tempfile::tempdir().expect("temporary project directory");
    let project_path = temp.path().join("atomic.pcba");
    let original = ProjectBundle::new(AtelierDocument::new_card("原工程", 64_000, 100_000));
    original.save(&project_path).expect("save original");
    let before = std::fs::read(&project_path).expect("original bytes");

    let mut invalid = original.clone();
    invalid.document.board = atelier_core::BoardOutline::Rectangle {
        width_um: 0,
        height_um: 100_000,
    };
    let error = invalid
        .save(&project_path)
        .expect_err("invalid replacement must fail");
    assert!(error.to_string().contains("document.board"));
    assert_eq!(
        std::fs::read(&project_path).expect("project remains readable"),
        before
    );
}

fn write_archive(path: &std::path::Path, manifest: &serde_json::Value, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).expect("create archive");
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .expect("start manifest");
    archive
        .write_all(serde_json::to_string_pretty(manifest).unwrap().as_bytes())
        .expect("write manifest");
    for (entry_path, bytes) in entries {
        archive
            .start_file(*entry_path, zip::write::SimpleFileOptions::default())
            .expect("start extra entry");
        archive.write_all(bytes).expect("write extra entry");
    }
    archive.finish().expect("finish archive");
}
