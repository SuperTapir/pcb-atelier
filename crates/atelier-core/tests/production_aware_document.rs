use atelier_core::{
    AtelierDocument, CardSide, CharacterProcess, CombineMode, ContentLayer, CopperWeight,
    FaceProductionLayer, ImageProductionMode, ImageTreatment, ManufacturerProfileSnapshot,
    ProductionMapping, ProductionTarget, SolderMaskColor, SurfaceFinish, TransformUm,
    TreatmentRecipe,
};

#[test]
fn image_mapping_references_a_versioned_treatment_for_the_same_asset() {
    let mut document = AtelierDocument::new_card("处理版本", 64_000, 100_000);
    let asset_id = atelier_core::AssetId::new();
    let other_asset_id = atelier_core::AssetId::new();
    let treatment = ImageTreatment::new(asset_id, TreatmentRecipe::standard_monochrome());
    let treatment_id = treatment.id;
    document.image_treatments.push(treatment);

    let image = ContentLayer::new_image(
        "Logo",
        asset_id,
        TransformUm::rect(1_000, 2_000, 20_000, 10_000),
    );
    let layer_id = image.id;
    document.front.layers.push(image);
    let mut mapping = ProductionMapping::new(
        layer_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Copper),
        CombineMode::Add,
    );
    mapping.treatment_id = Some(treatment_id);
    document.mappings.push(mapping);

    document.assets.extend([
        atelier_core::ProjectAsset::fixture(asset_id, "logo.png", &"a".repeat(64)),
        atelier_core::ProjectAsset::fixture(other_asset_id, "other.png", &"b".repeat(64)),
    ]);
    document.validate().expect("valid treatment reference");

    document.image_treatments[0].asset_id = other_asset_id;
    let error = document
        .validate()
        .expect_err("mapping treatment must belong to its image asset");
    assert!(error.to_string().contains("treatment"));
}

#[test]
fn manufacturer_snapshot_uses_stable_semantics_and_rejects_invalid_color_process() {
    let mut profile = ManufacturerProfileSnapshot::jlcpcb_fr4_2026_04();
    assert_eq!(profile.profile_version, "jlcpcb-fr4-art-v2026.04");
    assert_eq!(profile.outer_copper, CopperWeight::Oz1);
    assert_eq!(profile.solder_mask, SolderMaskColor::Blue);
    assert_eq!(profile.surface_finish, SurfaceFinish::Enig);

    let document = AtelierDocument::new_card("默认工艺", 64_000, 100_000);
    assert_eq!(document.stackup.solder_mask_color, SolderMaskColor::Blue);
    assert_eq!(document.stackup.surface_finish, SurfaceFinish::Enig);

    profile.solder_mask = SolderMaskColor::Purple;
    profile.character_process = CharacterProcess::StandardWhite;
    profile
        .validate()
        .expect("purple supports white standard ink");

    profile.character_process = CharacterProcess::Multicolor;
    let errors = profile
        .validate()
        .expect_err("multicolor requires the documented manufacturing combination");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("white solder mask"))
    );

    profile.solder_mask = SolderMaskColor::White;
    profile.surface_finish = SurfaceFinish::Enig;
    profile
        .validate()
        .expect("documented multicolor combination");
}

#[test]
fn manufacturer_snapshot_rejects_unknown_persisted_fields() {
    let mut value = serde_json::to_value(ManufacturerProfileSnapshot::jlcpcb_fr4_2026_04())
        .expect("serialize manufacturer snapshot");
    value["legacyBoardColor"] = serde_json::json!("black");
    let error = serde_json::from_value::<ManufacturerProfileSnapshot>(value)
        .expect_err("legacy manufacturer field must be rejected");
    assert!(error.to_string().contains("legacyBoardColor"));
}

#[test]
fn manufacturer_snapshot_identity_must_exactly_match_the_current_snapshot() {
    let current = ManufacturerProfileSnapshot::jlcpcb_fr4_2026_04();
    let cases = [
        (
            ManufacturerProfileSnapshot {
                manufacturer_id: "other".to_owned(),
                ..current.clone()
            },
            "manufacturer id",
        ),
        (
            ManufacturerProfileSnapshot {
                profile_version: "jlcpcb-fr4-art-v2025.01".to_owned(),
                ..current.clone()
            },
            "profile version",
        ),
        (
            ManufacturerProfileSnapshot {
                source_updated_at: "2025-01-01".to_owned(),
                ..current.clone()
            },
            "source update date",
        ),
        (
            ManufacturerProfileSnapshot {
                source_urls: vec!["https://legacy.invalid/capabilities".to_owned()],
                ..current
            },
            "source URLs",
        ),
    ];

    for (profile, expected) in cases {
        let errors = profile
            .validate()
            .expect_err("old or foreign snapshot identity must be rejected");
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "expected {expected} error, got {errors:?}"
        );
    }
}

#[test]
fn jlcpcb_fr4_snapshot_lists_only_documented_common_outer_copper_weights() {
    let profile = ManufacturerProfileSnapshot::jlcpcb_fr4_2026_04();
    assert_eq!(profile.source_updated_at, "2026-04-14");
    assert_eq!(
        profile.source_urls,
        [
            "https://jlcpcb.com/help/article/how-to-design-multi-color-silkscreen-using-easyeda",
            "https://jlcpcb.com/capabilities/Capabilities",
            "https://jlcpcb.com/help/article/jlcpcb-copper-weight",
            "https://jlcpcb.com/help/article/jlcpcb-surface-finish",
        ]
    );
    assert_eq!(
        ManufacturerProfileSnapshot::supported_outer_copper_weights(),
        &[CopperWeight::Oz1, CopperWeight::Oz2]
    );
    assert_eq!(
        ManufacturerProfileSnapshot::supported_surface_finishes(),
        &[
            SurfaceFinish::HaslLead,
            SurfaceFinish::HaslLeadFree,
            SurfaceFinish::Enig,
        ]
    );

    let mut profile = ManufacturerProfileSnapshot::jlcpcb_fr4_2026_04();
    profile.outer_copper = CopperWeight::Oz0_5;
    let errors = profile
        .validate()
        .expect_err("0.5 oz is an inner-layer weight, not a supported outer-layer option");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("outer copper weight"))
    );
}

#[test]
fn jlcpcb_fr4_snapshot_rejects_values_outside_its_versioned_capability_lists() {
    let mut profile = ManufacturerProfileSnapshot::jlcpcb_fr4_2026_04();
    profile.layer_count = 3;
    profile.thickness_um = 1_500;
    profile.surface_finish = SurfaceFinish::Osp;

    let errors = profile
        .validate()
        .expect_err("unsupported manufacturing values must fail before export");
    assert!(errors.iter().any(|error| error.contains("layer count")));
    assert!(errors.iter().any(|error| error.contains("board thickness")));
    assert!(errors.iter().any(|error| error.contains("surface finish")));
}

#[test]
fn jlcpcb_multicolor_silkscreen_accepts_only_documented_layer_counts() {
    let mut profile = ManufacturerProfileSnapshot::jlcpcb_fr4_2026_04();
    profile.solder_mask = SolderMaskColor::White;
    profile.character_process = CharacterProcess::Multicolor;
    profile.surface_finish = SurfaceFinish::Enig;
    profile.outer_copper = CopperWeight::Oz1;

    for layer_count in [2, 4] {
        profile.layer_count = layer_count;
        profile
            .validate()
            .expect("JLCPCB documents 2- and 4-layer multicolor boards");
    }

    profile.layer_count = 6;
    let errors = profile
        .validate()
        .expect_err("multicolor silkscreen does not support 6-layer boards");
    assert!(errors.iter().any(|error| error.contains("2 or 4 layers")));
}

#[test]
fn color_original_treatment_requires_supported_media_multicolor_profile_and_silkscreen_mapping() {
    let mut document = AtelierDocument::new_card("彩色原图丝印", 64_000, 100_000);
    let asset_id = atelier_core::AssetId::new();
    document.assets.push(atelier_core::AssetReference {
        id: asset_id,
        embedded_path: format!("assets/{asset_id}.png"),
        original_filename: "portrait.png".to_owned(),
        media_type: "image/png".to_owned(),
        sha256: "a".repeat(64),
        pixel_width: 1_200,
        pixel_height: 800,
        folder_path: None,
        tags: Vec::new(),
        has_alpha: true,
    });
    let mut treatment = ImageTreatment::new(asset_id, TreatmentRecipe::standard_monochrome());
    treatment.production_mode = ImageProductionMode::ColorOriginal;
    let treatment_id = treatment.id;
    document.image_treatments.push(treatment);
    let image = ContentLayer::new_image(
        "彩色人物",
        asset_id,
        TransformUm::rect(2_000, 3_000, 40_000, 30_000),
    );
    let layer_id = image.id;
    document.front.layers.push(image);
    let mut mapping = ProductionMapping::new(
        layer_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
        CombineMode::Add,
    );
    mapping.treatment_id = Some(treatment_id);
    document.mappings.push(mapping);

    let error = document
        .validate()
        .expect_err("standard character process cannot consume color originals");
    assert!(error.to_string().contains("启用彩色丝印"));

    document.stackup.solder_mask_color = SolderMaskColor::White;
    document.manufacturer_profile.solder_mask = SolderMaskColor::White;
    document.manufacturer_profile.character_process = CharacterProcess::Multicolor;
    document
        .validate()
        .expect("PNG on silkscreen is valid for the supported multicolor profile");

    document.mappings[0].target.layer = FaceProductionLayer::Copper;
    let error = document
        .validate()
        .expect_err("color original cannot target copper");
    assert!(error.to_string().contains("silkscreen"));

    document.mappings[0].target.layer = FaceProductionLayer::Silkscreen;
    document.assets[0].media_type = "image/svg+xml".to_owned();
    let error = document
        .validate()
        .expect_err("color original accepts only PNG and JPEG");
    assert!(error.to_string().contains("PNG") && error.to_string().contains("JPEG"));
}

#[test]
fn image_production_mode_is_a_required_current_schema_field() {
    let treatment = ImageTreatment::new(
        atelier_core::AssetId::new(),
        TreatmentRecipe::standard_monochrome(),
    );
    assert_eq!(
        treatment.production_mode,
        ImageProductionMode::MonochromeMask
    );

    let mut json = serde_json::to_value(&treatment).expect("serialize treatment");
    json.as_object_mut()
        .expect("treatment object")
        .remove("productionMode");
    assert!(
        serde_json::from_value::<ImageTreatment>(json).is_err(),
        "unreleased old treatment payload must not be inferred"
    );
}
