use std::io::Cursor;

use atelier_core::{
    AlphaMode, CropRect, SamplingPurpose, ThinFeaturePolicy, ThresholdMode, TreatmentCompileError,
    TreatmentCompileRequest, TreatmentDiagnostic, TreatmentRecipe, compile_image_treatment,
    compile_prepared_image, compile_prepared_image_with_cancel, prepare_image, treatment_cache_key,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

fn fixture_png() -> Vec<u8> {
    let mut image = RgbaImage::from_pixel(8, 8, Rgba([255, 255, 255, 255]));
    for y in 1..=5 {
        for x in 1..=3 {
            image.put_pixel(x, y, Rgba([20, 20, 20, 255]));
        }
    }
    image.put_pixel(6, 1, Rgba([0, 0, 0, 255]));
    image.put_pixel(7, 7, Rgba([0, 0, 0, 0]));
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("fixture PNG");
    bytes.into_inner()
}

#[test]
fn prepared_source_matches_the_bytes_entry_and_can_be_cancelled() {
    let bytes = fixture_png();
    let recipe = TreatmentRecipe::default();
    let request =
        TreatmentCompileRequest::for_purpose(51_170, 80_000, 7, SamplingPurpose::InteractiveProxy);
    let prepared = prepare_image(&bytes).expect("prepare source once");
    let from_bytes = compile_image_treatment(&bytes, &recipe, request).expect("bytes entry");
    let from_prepared =
        compile_prepared_image(&prepared, &recipe, request).expect("prepared entry");

    assert_eq!(from_prepared, from_bytes);
    assert_eq!(prepared.source_sha256().len(), 64);
    assert!(prepared.estimated_bytes() > 0);

    let mut probes = 0;
    let cancelled = compile_prepared_image_with_cancel(&prepared, &recipe, request, || {
        probes += 1;
        probes >= 2
    });
    assert!(matches!(cancelled, Err(TreatmentCompileError::Cancelled)));
    assert_eq!(
        compile_prepared_image(&prepared, &recipe, request).expect("deterministic retry"),
        from_bytes
    );
}

#[test]
fn otsu_recipe_fingerprint_matches_the_web_contract() {
    let mut recipe = TreatmentRecipe::default();
    recipe.threshold = ThresholdMode::Otsu;
    recipe.smoothing_radius_um = 500;
    assert_eq!(
        recipe.fingerprint(),
        "125c4247000170252ef09a847fe1b53b76d80ad3ca3bdfa38a6632a919629156"
    );
}

fn encode_png(image: RgbaImage) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("fixture PNG");
    bytes.into_inner()
}

fn encode_oriented_jpeg(image: RgbaImage, exif_orientation: u16) -> Vec<u8> {
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut encoded, ImageFormat::Jpeg)
        .expect("fixture JPEG");
    let jpeg = encoded.into_inner();
    assert_eq!(&jpeg[..2], &[0xff, 0xd8]);

    let mut exif = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
    exif.extend_from_slice(&exif_orientation.to_le_bytes());
    exif.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let segment_length = u16::try_from(exif.len() + 2).unwrap();
    let mut result = Vec::with_capacity(jpeg.len() + exif.len() + 4);
    result.extend_from_slice(&jpeg[..2]);
    result.extend_from_slice(&[0xff, 0xe1]);
    result.extend_from_slice(&segment_length.to_be_bytes());
    result.extend_from_slice(&exif);
    result.extend_from_slice(&jpeg[2..]);
    result
}

#[test]
fn manual_otsu_alpha_and_invert_are_reproducible_recipe_semantics() {
    let bytes = fixture_png();
    let request = TreatmentCompileRequest {
        physical_width_um: 8_000,
        physical_height_um: 8_000,
        pixel_pitch_um: 1_000,
        revision: 7,
        purpose: SamplingPurpose::FormalProduction,
    };
    let mut recipe = TreatmentRecipe::standard_monochrome();
    let manual = compile_image_treatment(&bytes, &recipe, request).expect("manual treatment");
    assert!(manual.mask.get(1, 1).unwrap());
    assert!(!manual.mask.get(0, 0).unwrap());
    assert!(
        !manual.mask.get(7, 7).unwrap(),
        "transparent black stays empty"
    );

    recipe.threshold = ThresholdMode::Otsu;
    let automatic = compile_image_treatment(&bytes, &recipe, request).expect("Otsu treatment");
    let mut frozen_recipe = recipe.clone();
    frozen_recipe.threshold = ThresholdMode::Manual {
        value: automatic.applied_threshold,
    };
    let frozen =
        compile_image_treatment(&bytes, &frozen_recipe, request).expect("frozen Otsu threshold");
    assert_eq!(
        automatic.mask.sha256(),
        frozen.mask.sha256(),
        "Otsu 公开的实际阈值必须能固化为可编辑且结果相同的手动初始值"
    );
    assert_eq!(automatic.topology.island_count, 2);
    assert_eq!(automatic.recipe_fingerprint, recipe.fingerprint());
    assert_eq!(automatic.revision, 7);

    recipe.invert = true;
    let inverted = compile_image_treatment(&bytes, &recipe, request).expect("inverted treatment");
    assert_ne!(automatic.mask.sha256(), inverted.mask.sha256());
    assert!(inverted.mask.get(0, 0).unwrap());

    recipe.alpha_mode = AlphaMode::IgnoreAlpha;
    let ignored_alpha =
        compile_image_treatment(&bytes, &recipe, request).expect("ignore alpha treatment");
    assert!(!ignored_alpha.mask.get(7, 7).unwrap());
}

#[test]
fn decode_normalizes_exif_orientation_before_crop_and_sampling() {
    let source = RgbaImage::from_fn(6, 4, |x, y| {
        if x < 2 && y < 2 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    });
    let oriented = encode_oriented_jpeg(source, 6);
    let mut recipe = TreatmentRecipe::standard_monochrome();
    recipe.threshold = ThresholdMode::Manual { value: 80 };
    let output = compile_image_treatment(
        &oriented,
        &recipe,
        TreatmentCompileRequest {
            physical_width_um: 4_000,
            physical_height_um: 6_000,
            pixel_pitch_um: 1_000,
            revision: 9,
            purpose: SamplingPurpose::FormalProduction,
        },
    )
    .unwrap();

    assert_eq!((output.mask.width_px(), output.mask.height_px()), (4, 6));
    assert!(output.mask.get(3, 0).unwrap());
    assert!(!output.mask.get(0, 0).unwrap());
    assert!(!output.mask.get(3, 5).unwrap());
}

#[test]
fn physical_island_cleanup_is_stable_across_sampling_tiers() {
    let bytes = fixture_png();
    let mut recipe = TreatmentRecipe::standard_monochrome();
    recipe.remove_islands_below_um2 = 2_000_000;
    let interactive = compile_image_treatment(
        &bytes,
        &recipe,
        TreatmentCompileRequest::for_purpose(8_000, 8_000, 11, SamplingPurpose::InteractiveProxy),
    )
    .expect("interactive treatment");
    let production = compile_image_treatment(
        &bytes,
        &recipe,
        TreatmentCompileRequest::for_purpose(8_000, 8_000, 11, SamplingPurpose::FormalProduction),
    )
    .expect("production treatment");
    let board_preview = compile_image_treatment(
        &bytes,
        &recipe,
        TreatmentCompileRequest::for_purpose(8_000, 8_000, 11, SamplingPurpose::BoardPreview),
    )
    .expect("board preview treatment");

    assert_eq!(interactive.topology.island_count, 1);
    assert_eq!(board_preview.topology.island_count, 1);
    assert_eq!(production.topology.island_count, 1);
    assert_eq!(
        interactive.recipe_fingerprint,
        production.recipe_fingerprint
    );
    assert_eq!(interactive.bounds_um, production.bounds_um);
}

#[test]
fn physical_despeckle_and_gap_diagnostics_ignore_unbounded_background() {
    let bytes = fixture_png();
    let mut recipe = TreatmentRecipe::standard_monochrome();
    recipe.despeckle_radius_um = 1_000;
    let cleaned = compile_image_treatment(
        &bytes,
        &recipe,
        TreatmentCompileRequest {
            physical_width_um: 8_000,
            physical_height_um: 8_000,
            pixel_pitch_um: 1_000,
            revision: 12,
            purpose: SamplingPurpose::FormalProduction,
        },
    )
    .unwrap();
    assert_eq!(cleaned.topology.island_count, 1);
    assert!(cleaned.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        TreatmentDiagnostic::RemovedSpeck { diameter_um: 1_000 }
    )));

    let gap_fixture = encode_png(RgbaImage::from_fn(7, 5, |x, y| {
        if (x <= 2 || x >= 4) && (1..=3).contains(&y) {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    }));
    recipe.despeckle_radius_um = 0;
    recipe.minimum_gap_um = 2_000;
    let with_gap = compile_image_treatment(
        &gap_fixture,
        &recipe,
        TreatmentCompileRequest {
            physical_width_um: 7_000,
            physical_height_um: 5_000,
            pixel_pitch_um: 1_000,
            revision: 13,
            purpose: SamplingPurpose::FormalProduction,
        },
    )
    .unwrap();
    assert!(with_gap.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        TreatmentDiagnostic::GapBelowMinimum { minimum_um: 2_000 }
    )));

    recipe.minimum_gap_um = 1_000;
    let allowed_gap = compile_image_treatment(
        &gap_fixture,
        &recipe,
        TreatmentCompileRequest {
            physical_width_um: 7_000,
            physical_height_um: 5_000,
            pixel_pitch_um: 1_000,
            revision: 14,
            purpose: SamplingPurpose::FormalProduction,
        },
    )
    .unwrap();
    assert!(
        !allowed_gap
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic, TreatmentDiagnostic::GapBelowMinimum { .. }))
    );
}

#[test]
fn cache_key_ignores_instance_position_and_rotation() {
    let recipe = TreatmentRecipe::standard_monochrome();
    let first = treatment_cache_key("asset-hash", &recipe, 20_000, 10_000, 100);
    let second = treatment_cache_key("asset-hash", &recipe, 20_000, 10_000, 100);
    assert_eq!(first, second);
    assert_ne!(
        first,
        treatment_cache_key("asset-hash", &recipe, 21_000, 10_000, 100)
    );
}

#[test]
fn versioned_interpreter_rejects_unknown_algorithm_semantics() {
    let mut recipe = TreatmentRecipe::standard_monochrome();
    recipe.algorithm_version = "atelier-image-treatment-v999".to_owned();
    assert!(matches!(
        recipe.validate(),
        Err(atelier_core::TreatmentRecipeValidationError::UnsupportedAlgorithmVersion(
            version
        )) if version == "atelier-image-treatment-v999"
    ));
    assert!(matches!(
        compile_image_treatment(
            &fixture_png(),
            &recipe,
            TreatmentCompileRequest::for_purpose(
                8_000,
                8_000,
                1,
                SamplingPurpose::FormalProduction,
            ),
        ),
        Err(TreatmentCompileError::UnsupportedAlgorithmVersion(version))
            if version == "atelier-image-treatment-v999"
    ));
}

#[test]
fn persisted_treatment_facts_reject_unknown_fields() {
    let recipe = TreatmentRecipe::standard_monochrome();
    let mut recipe_json = serde_json::to_value(&recipe).expect("serialize recipe");
    recipe_json["legacyThreshold"] = serde_json::json!(128);
    let error = serde_json::from_value::<TreatmentRecipe>(recipe_json)
        .expect_err("legacy recipe field must be rejected");
    assert!(error.to_string().contains("legacyThreshold"));

    let treatment = atelier_core::ImageTreatment::new(
        atelier_core::AssetId::new(),
        TreatmentRecipe::standard_monochrome(),
    );
    let mut treatment_json = serde_json::to_value(&treatment).expect("serialize treatment");
    treatment_json["cachedPreview"] = serde_json::json!("legacy.png");
    let error = serde_json::from_value::<atelier_core::ImageTreatment>(treatment_json)
        .expect_err("derived treatment field must be rejected");
    assert!(error.to_string().contains("cachedPreview"));

    let threshold = ThresholdMode::Manual { value: 128 };
    let mut threshold_json = serde_json::to_value(threshold).expect("serialize threshold");
    threshold_json["legacyBias"] = serde_json::json!(4);
    let error = serde_json::from_value::<ThresholdMode>(threshold_json)
        .expect_err("legacy threshold payload must be rejected");
    assert!(error.to_string().contains("legacyBias"));
}

#[test]
fn recipe_validation_rejects_invalid_normalized_crop_numbers() {
    for crop in [
        CropRect {
            x_millionths: 0,
            y_millionths: 0,
            width_millionths: 0,
            height_millionths: 1,
        },
        CropRect {
            x_millionths: 900_000,
            y_millionths: 0,
            width_millionths: 100_001,
            height_millionths: 1,
        },
        CropRect {
            x_millionths: u32::MAX,
            y_millionths: 0,
            width_millionths: 2,
            height_millionths: 1,
        },
    ] {
        let mut recipe = TreatmentRecipe::standard_monochrome();
        recipe.crop = Some(crop);
        assert!(matches!(
            recipe.validate(),
            Err(atelier_core::TreatmentRecipeValidationError::InvalidCrop)
        ));
    }
}

#[test]
fn alpha_modes_crop_and_smoothing_have_distinct_reproducible_semantics() {
    let alpha_fixture = encode_png(
        RgbaImage::from_raw(
            3,
            1,
            vec![
                255, 255, 255, 128, // white at half coverage
                0, 0, 0, 0, // transparent black
                0, 0, 0, 255, // opaque black
            ],
        )
        .unwrap(),
    );
    let request = TreatmentCompileRequest {
        physical_width_um: 3_000,
        physical_height_um: 1_000,
        pixel_pitch_um: 1_000,
        revision: 1,
        purpose: SamplingPurpose::FormalProduction,
    };
    let mut recipe = TreatmentRecipe::standard_monochrome();
    recipe.threshold = ThresholdMode::Manual { value: 128 };

    recipe.alpha_mode = AlphaMode::CompositeOnWhite;
    let composite = compile_image_treatment(&alpha_fixture, &recipe, request).unwrap();
    recipe.alpha_mode = AlphaMode::AlphaAsCoverage;
    let coverage = compile_image_treatment(&alpha_fixture, &recipe, request).unwrap();
    recipe.alpha_mode = AlphaMode::IgnoreAlpha;
    let ignored = compile_image_treatment(&alpha_fixture, &recipe, request).unwrap();

    assert!(!composite.mask.get(0, 0).unwrap());
    assert!(coverage.mask.get(0, 0).unwrap());
    assert!(!ignored.mask.get(0, 0).unwrap());
    assert!(!coverage.mask.get(1, 0).unwrap());
    assert!(ignored.mask.get(1, 0).unwrap());

    let crop_fixture = encode_png(RgbaImage::from_fn(4, 1, |x, _| {
        if x == 3 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    }));
    recipe.alpha_mode = AlphaMode::CompositeOnWhite;
    recipe.crop = Some(CropRect {
        x_millionths: 750_000,
        y_millionths: 0,
        width_millionths: 250_000,
        height_millionths: 1_000_000,
    });
    let cropped = compile_image_treatment(&crop_fixture, &recipe, request).unwrap();
    assert_eq!(cropped.mask.active_pixel_count(), 3);

    recipe.crop = Some(CropRect {
        x_millionths: 900_000,
        y_millionths: 0,
        width_millionths: 200_000,
        height_millionths: 1_000_000,
    });
    assert!(matches!(
        compile_image_treatment(&crop_fixture, &recipe, request),
        Err(TreatmentCompileError::InvalidCrop)
    ));

    let smoothing_fixture = encode_png(RgbaImage::from_fn(3, 3, |x, y| {
        if (x, y) == (1, 1) {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    }));
    recipe.crop = None;
    recipe.smoothing_radius_um = 1_000;
    let smoothed = compile_image_treatment(
        &smoothing_fixture,
        &recipe,
        TreatmentCompileRequest {
            physical_width_um: 3_000,
            physical_height_um: 3_000,
            ..request
        },
    )
    .unwrap();
    assert_eq!(smoothed.mask.active_pixel_count(), 0);
}

#[test]
fn thin_feature_policy_explicitly_preserves_thickens_or_removes_risky_lines() {
    let line = encode_png(RgbaImage::from_fn(10, 10, |x, _| {
        if x == 4 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    }));
    let request = TreatmentCompileRequest {
        physical_width_um: 1_000,
        physical_height_um: 1_000,
        pixel_pitch_um: 100,
        revision: 2,
        purpose: SamplingPurpose::FormalProduction,
    };
    let mut recipe = TreatmentRecipe::standard_monochrome();
    recipe.minimum_line_width_um = 300;

    recipe.thin_feature_policy = ThinFeaturePolicy::Preserve;
    let preserved = compile_image_treatment(&line, &recipe, request).unwrap();
    assert_eq!(preserved.mask.active_pixel_count(), 10);
    assert!(preserved.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        TreatmentDiagnostic::FeatureBelowMinimumLineWidth { .. }
    )));

    recipe.thin_feature_policy = ThinFeaturePolicy::Thicken;
    let thickened = compile_image_treatment(&line, &recipe, request).unwrap();
    assert!(thickened.mask.active_pixel_count() >= 30);
    assert!(
        thickened.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            TreatmentDiagnostic::ThickenedThinFeature { .. }
        ))
    );

    recipe.thin_feature_policy = ThinFeaturePolicy::Remove;
    let removed = compile_image_treatment(&line, &recipe, request).unwrap();
    assert_eq!(removed.mask.active_pixel_count(), 0);
    assert_eq!(removed.topology.island_count, 0);
    assert!(
        removed
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic, TreatmentDiagnostic::RemovedThinFeature { .. }))
    );
}

#[test]
fn thin_feature_repair_keeps_topology_and_physical_width_across_sampling_tiers() {
    let line = encode_png(RgbaImage::from_fn(40, 40, |x, y| {
        if (18..=19).contains(&x) && (4..=35).contains(&y) {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    }));
    let mut recipe = TreatmentRecipe::standard_monochrome();
    recipe.minimum_line_width_um = 750;
    recipe.thin_feature_policy = ThinFeaturePolicy::Thicken;
    let outputs = [
        SamplingPurpose::InteractiveProxy,
        SamplingPurpose::BoardPreview,
        SamplingPurpose::FormalProduction,
    ]
    .map(|purpose| {
        compile_image_treatment(
            &line,
            &recipe,
            TreatmentCompileRequest::for_purpose(10_000, 10_000, 21, purpose),
        )
        .unwrap()
    });

    for output in &outputs {
        assert_eq!(output.topology.island_count, 1);
        assert_eq!(output.topology.hole_count, 0);
        assert!(output.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            TreatmentDiagnostic::ThickenedThinFeature { .. }
        )));
    }
    let extents = outputs
        .iter()
        .map(active_physical_bounds)
        .collect::<Vec<_>>();
    for candidate in &extents[1..] {
        assert!(candidate[0].abs_diff(extents[0][0]) <= 250);
        assert!(candidate[2].abs_diff(extents[0][2]) <= 250);
    }
}

#[test]
fn three_sampling_tiers_preserve_asymmetric_direction_polarity_and_topology() {
    let golden = encode_png(RgbaImage::from_fn(40, 40, |x, y| {
        let outer = (4..=30).contains(&x) && (5..=34).contains(&y);
        let hole = (12..=20).contains(&x) && (14..=24).contains(&y);
        let direction_mark = (32..=36).contains(&x) && (6..=10).contains(&y);
        if (outer && !hole) || direction_mark {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    }));
    let recipe = TreatmentRecipe::standard_monochrome();
    let outputs = [
        SamplingPurpose::InteractiveProxy,
        SamplingPurpose::BoardPreview,
        SamplingPurpose::FormalProduction,
    ]
    .map(|purpose| {
        compile_image_treatment(
            &golden,
            &recipe,
            TreatmentCompileRequest::for_purpose(10_000, 10_000, 42, purpose),
        )
        .unwrap()
    });

    for output in &outputs {
        assert_eq!(output.recipe_fingerprint, recipe.fingerprint());
        assert_eq!(output.revision, 42);
        assert_eq!(output.bounds_um.min_x_um, 0);
        assert_eq!(output.bounds_um.min_y_um, 0);
        assert_eq!(output.bounds_um.max_x_um, 10_000);
        assert_eq!(output.bounds_um.max_y_um, 10_000);
        assert_eq!(output.topology.island_count, 2);
        assert_eq!(output.topology.hole_count, 1);
        assert!(sample_physical(output, 8_500, 2_000));
        assert!(!sample_physical(output, 500, 2_000));
        assert!(!sample_physical(output, 4_000, 5_000));
    }
    let physical_extents = outputs
        .iter()
        .map(active_physical_bounds)
        .collect::<Vec<_>>();
    for extents in &physical_extents[1..] {
        for (actual, proxy) in extents.iter().zip(physical_extents[0]) {
            assert!(
                actual.abs_diff(proxy)
                    <= SamplingPurpose::InteractiveProxy.default_pixel_pitch_um(),
                "三级采样的物理轮廓误差不得超过交互代理的一个像素"
            );
        }
    }

    let mut inverted_recipe = recipe.clone();
    inverted_recipe.invert = true;
    for purpose in [
        SamplingPurpose::InteractiveProxy,
        SamplingPurpose::BoardPreview,
        SamplingPurpose::FormalProduction,
    ] {
        let inverted = compile_image_treatment(
            &golden,
            &inverted_recipe,
            TreatmentCompileRequest::for_purpose(10_000, 10_000, 43, purpose),
        )
        .unwrap();
        assert!(!sample_physical(&inverted, 8_500, 2_000));
        assert!(sample_physical(&inverted, 500, 2_000));
    }
}

fn sample_physical(output: &atelier_core::CompiledImageTreatment, x_um: u32, y_um: u32) -> bool {
    let x = (x_um / output.pixel_pitch_um).min(output.mask.width_px() - 1);
    let y = (y_um / output.pixel_pitch_um).min(output.mask.height_px() - 1);
    output.mask.get(x, y).unwrap()
}

fn active_physical_bounds(output: &atelier_core::CompiledImageTreatment) -> [u32; 4] {
    let mut min_x = output.mask.width_px();
    let mut min_y = output.mask.height_px();
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..output.mask.height_px() {
        for x in 0..output.mask.width_px() {
            if output.mask.get(x, y).unwrap() {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + 1);
                max_y = max_y.max(y + 1);
            }
        }
    }
    [
        min_x * output.pixel_pitch_um,
        min_y * output.pixel_pitch_um,
        max_x * output.pixel_pitch_um,
        max_y * output.pixel_pitch_um,
    ]
}
