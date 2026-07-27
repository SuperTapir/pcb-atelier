mod support;

use std::{fs::File, io::Read};

use atelier_core::{
    ProjectBundleRasterizer, SamplingPurpose, compile_fabrication_plan,
    convert_easyeda_archive_to_native, export_public_archive, preflight_resolved_board,
    resolve_fabrication_plan, resolve_fabrication_plan_for_purpose,
    validate_easyeda_native_project, validate_public_archive,
};
use serde_json::Value;

use support::asymmetric_golden_card;

#[test]
fn fabrication_board_round_trips_through_the_real_native_eda_envelope() {
    let fixture = asymmetric_golden_card();
    let _non_symmetric_markers = (fixture.front_text_id, fixture.back_text_id);
    let plan =
        compile_fabrication_plan(&fixture.bundle.document).expect("compile golden card plan");
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle).expect("embedded font");
    let board = resolve_fabrication_plan_for_purpose(
        &plan,
        SamplingPurpose::FormalProduction,
        &mut rasterizer,
    )
    .expect("resolve formal masks");
    assert!(preflight_resolved_board(&board).can_export);
    let directory = tempfile::tempdir().expect("temporary directory");
    let public_path = directory.path().join("golden.epro2");
    let native_path = directory.path().join("golden.eprj2");

    let public = export_public_archive(&public_path, "Golden card", &board)
        .expect("write public EDA archive");
    let public_validation = validate_public_archive(&public_path).expect("read public archive");
    assert!(
        public_validation.is_valid,
        "{:#?}",
        public_validation.errors
    );
    assert_eq!(public_validation.board_width_um, 64_000);
    assert_eq!(public_validation.board_height_um, 100_000);
    assert_eq!(
        public_validation.fill_count, 6,
        "each populated production layer should be one editable EasyEDA artwork object"
    );
    assert_eq!(public_validation.hole_count, 1);
    assert_eq!(public_validation.filled_layer_ids, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(public.epru_file, "golden.epru");

    let native = convert_easyeda_archive_to_native(&public_path, &native_path)
        .expect("convert public archive to native envelope");
    let native_validation =
        validate_easyeda_native_project(&native_path).expect("validate native envelope");
    assert!(
        native_validation.is_valid,
        "{:#?}",
        native_validation.errors
    );
    assert_eq!(native.title, "Golden card");
    assert_eq!(native.board_uuid, public.board_uuid);
    assert_eq!(native.pcb_uuid, public.pcb_uuid);
    assert_eq!(native_validation.table_count, 35);
    assert_eq!(native_validation.index_count, 49);
    assert_eq!(native_validation.board_uuids, vec![public.board_uuid]);
    assert_eq!(native_validation.pcb_uuids, vec![public.pcb_uuid]);
    assert_eq!(native_validation.board_width_um, 64_000);
    assert_eq!(native_validation.board_height_um, 100_000);
    assert_eq!(native_validation.fill_count, public.fill_count);
    assert_eq!(native_validation.hole_count, 1);
    assert_eq!(native_validation.filled_layer_ids, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(native_validation.solder_mask_opening_layer_ids, vec![5, 6]);
    let native_extent = |layer_id| {
        native_validation
            .layer_x_extents
            .iter()
            .find(|extent| extent.layer_id == layer_id)
            .expect("native layer extent")
    };
    let top_extent = native_extent(1);
    let bottom_extent = native_extent(2);
    assert!(bottom_extent.min_x_nano_mil > top_extent.max_x_nano_mil);

    let mut archive =
        zip::ZipArchive::new(File::open(public_path).expect("archive")).expect("valid zip");
    assert_eq!(archive.len(), 2);
    let mut epru = String::new();
    archive
        .by_name(&public.epru_file)
        .expect("EPRU entry")
        .read_to_string(&mut epru)
        .expect("read EPRU");
    let records = epru
        .lines()
        .map(|line| {
            let (head, data) = line.trim_end_matches('|').split_once("||").expect("record");
            (
                serde_json::from_str::<Value>(head).expect("record header"),
                serde_json::from_str::<Value>(data).expect("record data"),
            )
        })
        .collect::<Vec<_>>();
    let fills = records
        .iter()
        .filter(|(head, _)| head["type"] == "FILL")
        .collect::<Vec<_>>();
    assert_eq!(fills.len(), public.fill_count);
    for (_, fill) in &fills {
        let rings = fill["path"].as_array().expect("FILL path rings");
        assert!(!rings.is_empty());
        for ring in rings {
            let ring = ring.as_array().expect("linear ring");
            assert!(ring.len() >= 9);
            assert_eq!(ring[2], "L");
        }
    }
    let x_bounds_for_layer = |layer_id| {
        let points = fills
            .iter()
            .filter(|(_, fill)| fill["layerId"] == layer_id)
            .flat_map(|(_, fill)| fill["path"].as_array().expect("FILL paths"))
            .flat_map(|ring| {
                let ring = ring.as_array().expect("linear ring");
                std::iter::once(ring[0].as_f64().expect("first X")).chain(
                    ring[3..]
                        .chunks_exact(2)
                        .map(|pair| pair[0].as_f64().expect("line segment X")),
                )
            })
            .collect::<Vec<_>>();
        (
            points
                .iter()
                .copied()
                .reduce(f64::min)
                .expect("layer X minimum"),
            points
                .iter()
                .copied()
                .reduce(f64::max)
                .expect("layer X maximum"),
        )
    };
    let (top_copper_min_x, top_copper_max_x) = x_bounds_for_layer(1);
    let (bottom_copper_min_x, bottom_copper_max_x) = x_bounds_for_layer(2);
    assert!(top_copper_min_x < top_copper_max_x);
    assert!(bottom_copper_min_x < bottom_copper_max_x);
    // The asymmetric golden fixture puts bottom copper physically to the
    // right of top copper. A hidden bottom-side X mirror would violate this.
    assert!(bottom_copper_min_x > top_copper_max_x);
    let pads = records
        .iter()
        .filter(|(head, _)| head["type"] == "PAD")
        .collect::<Vec<_>>();
    assert_eq!(pads.len(), 1);
    assert_eq!(pads[0].1["plated"], false);
    assert!(!epru.contains("front.png"));
    assert!(!epru.contains("back.png"));
}

#[test]
fn public_export_preflight_rejects_incomplete_resolved_boards() {
    let fixture = asymmetric_golden_card();
    let plan =
        compile_fabrication_plan(&fixture.bundle.document).expect("compile golden card plan");
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle).expect("embedded font");
    let mut board = resolve_fabrication_plan(&plan, 500, &mut rasterizer).expect("resolve masks");
    board.layers.pop();

    let preflight = preflight_resolved_board(&board);
    assert!(!preflight.can_export);
    assert!(
        preflight
            .errors
            .iter()
            .any(|error| error.contains("six production layers"))
    );
}

#[test]
fn public_export_preflight_rejects_non_formal_resolved_boards() {
    let fixture = asymmetric_golden_card();
    let plan =
        compile_fabrication_plan(&fixture.bundle.document).expect("compile golden card plan");
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle).expect("embedded font");
    let board = resolve_fabrication_plan(&plan, 500, &mut rasterizer).expect("resolve preview");

    let preflight = preflight_resolved_board(&board);

    assert!(!preflight.can_export);
    assert!(
        preflight
            .errors
            .iter()
            .any(|error| error.contains("formalProduction"))
    );
}
