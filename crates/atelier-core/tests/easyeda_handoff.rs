mod support;

use std::{
    fs::File,
    io::Read,
    time::{Duration, Instant},
};

use atelier_core::{
    ProjectBundleRasterizer, SamplingPurpose, compile_fabrication_plan,
    convert_easyeda_archive_to_native, export_public_archive_with_document,
    preflight_resolved_board, resolve_fabrication_plan, resolve_fabrication_plan_for_purpose,
    validate_easyeda_native_project, validate_public_archive,
};
use serde_json::Value;

use support::asymmetric_golden_card;

#[test]
fn fabrication_board_round_trips_through_the_real_native_eda_envelope() {
    let mut fixture = asymmetric_golden_card();
    let back_text = fixture
        .bundle
        .document
        .back
        .layers
        .iter_mut()
        .find(|layer| layer.id == fixture.back_text_id)
        .expect("back text");
    let atelier_core::ContentKind::Text(content) = &mut back_text.kind else {
        panic!("back marker must be text");
    };
    content.text = "Kamome".to_owned();
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

    let back_text = fixture
        .bundle
        .document
        .back
        .layers
        .iter()
        .find(|layer| layer.id == fixture.back_text_id)
        .expect("back text");
    assert_eq!(
        back_text.kind,
        atelier_core::ContentKind::Text(atelier_core::TextContent {
            text: "Kamome".to_owned(),
            font_family: "sans-serif".to_owned(),
            font_size_um: 4_000,
            layout: atelier_core::TextLayout::AutoWidth,
        })
    );
    let export_started = Instant::now();
    let public = export_public_archive_with_document(
        &public_path,
        "Golden card",
        &fixture.bundle.document,
        &board,
    )
    .expect("write public EDA archive");
    assert!(
        export_started.elapsed() < Duration::from_secs(30),
        "formal native-artwork export exceeded the 30 second regression ceiling"
    );
    let public_validation = validate_public_archive(&public_path).expect("read public archive");
    assert!(
        public_validation.is_valid,
        "{:#?}",
        public_validation.errors
    );
    assert_eq!(public_validation.board_width_um, 64_000);
    assert_eq!(public_validation.board_height_um, 100_000);
    assert_eq!(public_validation.fill_count, 2);
    assert_eq!(public_validation.image_count, 4);
    assert_eq!(public_validation.string_count, 2);
    assert_eq!(
        public
            .layer_strategies
            .iter()
            .filter(|entry| entry.strategy == atelier_core::EasyedaArtworkStrategy::NativeImage)
            .count(),
        4
    );
    assert_eq!(
        public
            .layer_strategies
            .iter()
            .filter(|entry| entry.strategy == atelier_core::EasyedaArtworkStrategy::NativeString)
            .count(),
        2
    );
    assert!(
        public
            .layer_strategies
            .iter()
            .all(|entry| entry.fallback_reason.is_none())
    );
    assert!(
        public
            .layer_strategies
            .iter()
            .all(|entry| entry.exact_contour_fallbacks == 0),
        "golden IMAGE paths must not expose exact raster staircases"
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
    assert_eq!(native_validation.fill_count, 2);
    assert_eq!(native_validation.image_count, 4);
    assert_eq!(native_validation.string_count, 2);
    assert_eq!(native_validation.hole_count, 1);
    assert_eq!(native_validation.filled_layer_ids, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(native_validation.solder_mask_opening_layer_ids, vec![5, 6]);
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
    assert!(records.iter().any(|(head, data)| {
        head["type"] == "PRIMITIVE"
            && head["id"] == "[\"PRIMITIVE\",\"TEXT\"]"
            && data["display"] == true
            && data["pick"] == true
    }));
    assert!(records.iter().any(|(head, data)| {
        head["type"] == "LAYER"
            && head["id"] == "[\"LAYER\",4]"
            && data["layerType"] == "BOT_SILK"
            && data["use"] == true
            && data["show"] == true
    }));
    assert!(records.iter().any(|(head, data)| {
        head["type"] == "SILK_OPTS"
            && head["id"] == "[\"SILK_OPTS\",4]"
            && data["baseColor"] == "#FFFFFF"
    }));
    let images = records
        .iter()
        .filter(|(head, _)| head["type"] == "IMAGE")
        .collect::<Vec<_>>();
    assert_eq!(images.len(), 4);
    assert!(images.iter().all(|(_, image)| image["mirror"] == false));
    assert!(images.iter().all(|(_, image)| {
        image["path"]
            .as_array()
            .is_some_and(|paths| !paths.is_empty())
    }));
    let top_copper_image = images
        .iter()
        .find(|(_, image)| image["layerId"] == 1)
        .expect("top copper IMAGE");
    let bottom_copper_image = images
        .iter()
        .find(|(_, image)| image["layerId"] == 2)
        .expect("bottom copper IMAGE");
    assert_eq!(top_copper_image.1["startX"], -10_000.0 / 25.4);
    assert_eq!(
        bottom_copper_image.1["startX"],
        (64_000.0 - 26_000.0) / 25.4
    );
    let strings = records
        .iter()
        .filter(|(head, _)| head["type"] == "STRING")
        .collect::<Vec<_>>();
    assert_eq!(strings.len(), 2);
    let bottom_string = strings
        .iter()
        .find(|(_, string)| string["layerId"] == 4)
        .expect("bottom STRING");
    assert_eq!(bottom_string.1["text"], "Kamome");
    assert_eq!(
        bottom_string.1["text"]
            .as_str()
            .expect("STRING text")
            .chars()
            .count(),
        6
    );
    assert_eq!(bottom_string.1["mirror"], false);
    assert_eq!(bottom_string.1["reverse"], false);
    let string_id = bottom_string.0["id"].as_str().expect("STRING id");
    assert_eq!(string_id.len(), 16);
    assert!(
        string_id
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );
    assert!(
        bottom_string.0["firstTicket"]
            .as_u64()
            .expect("first ticket")
            < bottom_string.0["ticket"].as_u64().expect("current ticket"),
        "EasyEDA uses firstTicket as the earlier creation ticket, not the current ticket"
    );
    assert!(
        records.iter().all(|(head, _)| {
            head["ticket"] == bottom_string.0["ticket"]
                || head["ticket"] != bottom_string.0["firstTicket"]
        }),
        "STRING firstTicket is a reserved creation ticket and must not collide with another record"
    );
    assert_eq!(bottom_string.1["zIndex"], -1);
    assert_eq!(bottom_string.1["specialColor"], "#cc0066");
    assert_eq!(
        bottom_string.1["x"],
        (64_000.0 - back_text.transform.x_um as f64 - back_text.transform.width_um as f64) / 25.4,
        "back-side LEFT_BOTTOM anchor must include the text frame width"
    );
    assert_eq!(
        bottom_string.1["y"],
        (100_000.0 - 72_000.0 - 4_000.0) / 25.4,
        "LEFT_BOTTOM must use the text baseline, not the editor frame's top edge"
    );
    assert!(records.iter().any(|(head, data)| {
        head["type"] == "FILL"
            && data["layerId"] == 4
            && data["path"]
                .as_array()
                .is_some_and(|paths| !paths.is_empty())
    }));
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
fn reduced_easyeda_pro_baseline_documents_native_bottom_primitive_semantics() {
    let baseline: Value =
        serde_json::from_str(include_str!("fixtures/easyeda_native_artwork.json"))
            .expect("baseline fixture");

    assert_eq!(baseline["image"]["front"]["mirror"], false);
    assert_eq!(baseline["image"]["back"]["mirror"], false);
    assert!(
        baseline["image"]["back"]["startX"]
            .as_f64()
            .expect("back IMAGE anchor")
            > baseline["image"]["front"]["startX"]
                .as_f64()
                .expect("front IMAGE anchor")
    );
    assert_eq!(baseline["string"]["text"], "Kamome");
    assert_eq!(baseline["string"]["mirror"], false);
    assert_eq!(baseline["string"]["reverse"], false);
    assert_eq!(baseline["string"]["origin"], "LEFT_BOTTOM");
    assert_eq!(baseline["string"]["zIndex"], -1);
    assert_eq!(baseline["string"]["specialColor"], "#cc0066");
    assert_eq!(baseline["string"]["firstTicketRequired"], true);
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
