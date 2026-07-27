use atelier_core::{
    AtelierDocument, CardSide, CombineMode, ContentLayer, FaceProductionLayer, ImageTreatment,
    ProductionMapping, ProductionTarget, ProjectAssetCommand, ProjectAssetCommandOutcome,
    ProjectBundle, ProjectError, TransformUm, TreatmentRecipe,
};

#[test]
fn importing_identical_bytes_reuses_the_embedded_project_asset() {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("素材库", 64_000, 100_000));
    let first = bundle
        .embed_asset(
            "logo-original.png",
            "image/png",
            320,
            180,
            b"same-image".to_vec(),
        )
        .expect("first import");
    let second = bundle
        .embed_asset(
            "logo-copy.png",
            "image/png",
            320,
            180,
            b"same-image".to_vec(),
        )
        .expect("deduplicated import");

    assert_eq!(first, second);
    assert_eq!(bundle.document.assets.len(), 1);
    assert_eq!(bundle.assets.len(), 1);
    bundle.document.assets[0].folder_path = Some("品牌/标志".to_owned());
    bundle.document.assets[0].tags = vec!["hero".to_owned()];
    assert_eq!(bundle.search_assets("品牌").len(), 1);
    assert_eq!(bundle.search_assets("hero").len(), 1);
    assert_eq!(bundle.search_assets("missing").len(), 0);
}

#[test]
fn moving_an_asset_to_a_folder_preserves_content_identity_and_rejects_unsafe_paths() {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("素材整理", 64_000, 100_000));
    let asset_id = bundle
        .embed_asset("logo.png", "image/png", 320, 180, b"same-image".to_vec())
        .expect("asset");
    let original_hash = bundle.document.assets[0].sha256.clone();
    let original_bytes = bundle.asset_bytes(asset_id).expect("bytes").to_vec();

    let outcome = ProjectAssetCommand::MoveToFolder {
        asset_id,
        folder_path: Some(" 品牌 / 标志 ".to_owned()),
    }
    .apply(&mut bundle)
    .expect("move to folder");
    assert_eq!(
        outcome,
        ProjectAssetCommandOutcome::AssetMoved {
            asset_id,
            folder_path: Some("品牌/标志".to_owned()),
        }
    );
    assert_eq!(
        bundle.document.assets[0].folder_path.as_deref(),
        Some("品牌/标志")
    );
    assert_eq!(bundle.document.assets[0].sha256, original_hash);
    assert_eq!(
        bundle.asset_bytes(asset_id),
        Some(original_bytes.as_slice())
    );
    assert_eq!(bundle.search_assets("品牌").len(), 1);

    assert!(matches!(
        ProjectAssetCommand::MoveToFolder {
            asset_id,
            folder_path: Some("../外部".to_owned()),
        }
        .apply(&mut bundle),
        Err(ProjectError::InvalidAssetFolderPath(_))
    ));
    assert_eq!(
        bundle.document.assets[0].folder_path.as_deref(),
        Some("品牌/标志")
    );

    ProjectAssetCommand::MoveToFolder {
        asset_id,
        folder_path: Some("  ".to_owned()),
    }
    .apply(&mut bundle)
    .expect("move to uncategorized");
    assert_eq!(bundle.document.assets[0].folder_path, None);
}

#[test]
fn referenced_assets_are_protected_and_unused_assets_can_be_cleaned_explicitly() {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("素材生命周期", 64_000, 100_000));
    let used = bundle
        .embed_asset("used.png", "image/png", 10, 10, b"used".to_vec())
        .expect("used asset");
    let unused = bundle
        .embed_asset("unused.png", "image/png", 20, 20, b"unused".to_vec())
        .expect("unused asset");
    let treatment_only = bundle
        .embed_asset(
            "treatment-only.png",
            "image/png",
            20,
            20,
            b"treatment-only".to_vec(),
        )
        .expect("treatment-only asset");
    bundle.document.image_treatments.push(ImageTreatment::new(
        treatment_only,
        TreatmentRecipe::default(),
    ));
    let image = ContentLayer::new_image("Logo", used, TransformUm::rect(0, 0, 10_000, 10_000));
    let image_id = image.id;
    bundle.document.front.layers.push(image);
    bundle.document.mappings.push(ProductionMapping::new(
        image_id,
        ProductionTarget::new(CardSide::Front, FaceProductionLayer::Silkscreen),
        CombineMode::Add,
    ));

    assert_eq!(bundle.asset_usage_count(used), 1);
    assert_eq!(bundle.asset_usage_count(unused), 0);
    assert!(matches!(
        ProjectAssetCommand::Delete { asset_id: used }.apply(&mut bundle),
        Err(ProjectError::AssetInUse { asset_id, usage_count: 1 }) if asset_id == used
    ));
    assert!(matches!(
        ProjectAssetCommand::Delete {
            asset_id: treatment_only
        }
        .apply(&mut bundle),
        Err(ProjectError::AssetInUse { asset_id, usage_count: 1 }) if asset_id == treatment_only
    ));
    let outcome = ProjectAssetCommand::CleanupUnused
        .apply(&mut bundle)
        .expect("cleanup");
    assert_eq!(
        outcome,
        ProjectAssetCommandOutcome::UnusedAssetsRemoved {
            asset_ids: vec![unused]
        }
    );
    assert!(bundle.asset_bytes(used).is_some());
    assert!(bundle.asset_bytes(treatment_only).is_some());
    assert!(bundle.asset_bytes(unused).is_none());
}

#[test]
fn replacing_all_asset_references_preserves_instance_identity_and_transform() {
    let mut bundle = ProjectBundle::new(AtelierDocument::new_card("全局替换", 64_000, 100_000));
    let old = bundle
        .embed_asset("old.png", "image/png", 10, 10, b"old".to_vec())
        .expect("old asset");
    let replacement = bundle
        .embed_asset("new.png", "image/png", 10, 10, b"new".to_vec())
        .expect("new asset");
    let first = ContentLayer::new_image(
        "Logo 1",
        old,
        TransformUm::rect(1_000, 2_000, 10_000, 10_000),
    );
    let second = ContentLayer::new_image(
        "Logo 2",
        old,
        TransformUm::rect(20_000, 30_000, 8_000, 8_000),
    );
    let identities = [(first.id, first.transform), (second.id, second.transform)];
    bundle.document.front.layers.extend([first, second]);
    let treatment = ImageTreatment::new(old, TreatmentRecipe::default());
    let treatment_id = treatment.id;
    let treatment_recipe = treatment.recipe.clone();
    bundle.document.image_treatments.push(treatment);

    let outcome = ProjectAssetCommand::ReplaceAllReferences {
        original_asset_id: old,
        replacement_asset_id: replacement,
    }
    .apply(&mut bundle)
    .expect("replace all");
    assert_eq!(
        outcome,
        ProjectAssetCommandOutcome::ReferencesReplaced {
            original_asset_id: old,
            replacement_asset_id: replacement,
            instance_count: 2,
            treatment_count: 1,
        }
    );
    for (id, transform) in identities {
        let layer = bundle
            .document
            .front
            .layers
            .iter()
            .find(|layer| layer.id == id)
            .expect("stable instance");
        assert_eq!(layer.transform, transform);
        let atelier_core::ContentKind::Image(image) = &layer.kind else {
            panic!("image instance");
        };
        assert_eq!(image.asset_id, replacement);
    }
    let treatment = bundle
        .document
        .image_treatments
        .iter()
        .find(|treatment| treatment.id == treatment_id)
        .expect("stable treatment");
    assert_eq!(treatment.asset_id, replacement);
    assert_eq!(treatment.recipe, treatment_recipe);
}
