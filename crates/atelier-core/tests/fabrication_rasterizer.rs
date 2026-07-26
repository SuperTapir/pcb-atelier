use std::io::Cursor;

use atelier_core::{
    CombineMode, ContentLayer, CropRect, FabricationOperation, FabricationPrimitive,
    FabricationRasterizer, MappingId, ProjectBundle, ProjectBundleRasterizer, RasterGrid,
    TransformUm,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

fn png(pixels: &[(u32, u32, [u8; 4])], width: u32, height: u32) -> Vec<u8> {
    let mut image = RgbaImage::new(width, height);
    for &(x, y, color) in pixels {
        image.put_pixel(x, y, Rgba(color));
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encode png");
    bytes.into_inner()
}

fn operation(asset_id: atelier_core::AssetId, transform: TransformUm) -> FabricationOperation {
    FabricationOperation {
        mapping_id: MappingId::new(),
        source_layer_id: atelier_core::LayerId::new(),
        source_name: "image".to_owned(),
        combine: CombineMode::Add,
        transform,
        primitive: FabricationPrimitive::Image {
            asset_id,
            crop: None,
        },
        clip_to_board: true,
    }
}

#[test]
fn transparent_png_uses_the_fixed_alpha_threshold() {
    let mut bundle = ProjectBundle::new(atelier_core::AtelierDocument::new_card("alpha", 40, 20));
    let asset = bundle
        .embed_asset(
            "alpha.png",
            "image/png",
            2,
            1,
            png(&[(0, 0, [0, 0, 0, 127]), (1, 0, [0, 0, 0, 128])], 2, 1),
        )
        .expect("asset");
    let grid = RasterGrid::for_board(&bundle.document.board, 10).expect("grid");
    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("embedded CJK font");
    let mask = rasterizer
        .rasterize(&operation(asset, TransformUm::rect(0, 0, 20, 10)), &grid)
        .expect("rasterize");
    assert!(!mask.get(0, 0).expect("transparent pixel"));
    assert!(mask.get(1, 0).expect("threshold pixel"));
}

#[test]
fn opaque_white_background_is_not_treated_as_production_ink() {
    let mut bundle = ProjectBundle::new(atelier_core::AtelierDocument::new_card("tone", 20, 10));
    let asset = bundle
        .embed_asset(
            "tone.png",
            "image/png",
            2,
            1,
            png(
                &[(0, 0, [255, 255, 255, 255]), (1, 0, [0, 0, 0, 255])],
                2,
                1,
            ),
        )
        .expect("asset");
    let grid = RasterGrid::for_board(&bundle.document.board, 10).expect("grid");
    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("font");
    let mask = rasterizer
        .rasterize(&operation(asset, TransformUm::rect(0, 0, 20, 10)), &grid)
        .expect("rasterize");
    assert!(!mask.get(0, 0).expect("white background"));
    assert!(mask.get(1, 0).expect("black artwork"));
}

#[test]
fn image_crop_rotation_and_mirror_are_applied_before_board_clipping() {
    let mut bundle =
        ProjectBundle::new(atelier_core::AtelierDocument::new_card("transform", 40, 40));
    let asset = bundle
        .embed_asset(
            "source.png",
            "image/png",
            2,
            2,
            png(&[(0, 0, [0, 0, 0, 255]), (1, 1, [0, 0, 0, 255])], 2, 2),
        )
        .expect("asset");
    let grid = RasterGrid::for_board(&bundle.document.board, 10).expect("grid");
    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("font");
    let mut cropped = operation(
        asset,
        TransformUm {
            flip_x: true,
            rotation_mdeg: 90_000,
            ..TransformUm::rect(10, 10, 20, 20)
        },
    );
    cropped.primitive = FabricationPrimitive::Image {
        asset_id: asset,
        crop: Some(CropRect {
            x_millionths: 0,
            y_millionths: 0,
            width_millionths: 500_000,
            height_millionths: 500_000,
        }),
    };
    let mask = rasterizer.rasterize(&cropped, &grid).expect("rasterize");
    assert_eq!(
        mask.active_pixel_count(),
        4,
        "cropped opaque quadrant stays transformed"
    );
    assert_eq!(
        mask.sha256(),
        rasterizer
            .rasterize(&cropped, &grid)
            .expect("repeat")
            .sha256()
    );
}

#[test]
fn chinese_text_uses_embedded_noto_font_and_is_reproducible() {
    let bundle = ProjectBundle::new(atelier_core::AtelierDocument::new_card(
        "中文", 60_000, 30_000,
    ));
    let grid = RasterGrid::for_board(&bundle.document.board, 500).expect("grid");
    let text = ContentLayer::new_text(
        "中文",
        "你好",
        TransformUm::rect(2_000, 2_000, 30_000, 10_000),
    );
    let operation = FabricationOperation {
        mapping_id: MappingId::new(),
        source_layer_id: text.id,
        source_name: text.name,
        combine: CombineMode::Add,
        transform: text.transform,
        primitive: match text.kind {
            atelier_core::ContentKind::Text(value) => FabricationPrimitive::Text(value),
            _ => unreachable!(),
        },
        clip_to_board: true,
    };
    let mut first = ProjectBundleRasterizer::new(&bundle).expect("embedded CJK font");
    let first_mask = first
        .rasterize(&operation, &grid)
        .expect("Chinese rasterize");
    let mut second = ProjectBundleRasterizer::new(&bundle).expect("embedded CJK font");
    let second_mask = second
        .rasterize(&operation, &grid)
        .expect("repeat Chinese rasterize");
    assert!(first_mask.active_pixel_count() > 0);
    assert_eq!(first_mask.sha256(), second_mask.sha256());
    assert_eq!(first.font_fingerprint(), second.font_fingerprint());
}

#[test]
fn fixed_frame_text_wraps_and_clips_while_explicit_newlines_advance() {
    let bundle = ProjectBundle::new(atelier_core::AtelierDocument::new_card(
        "text frame",
        30_000,
        30_000,
    ));
    let grid = RasterGrid::for_board(&bundle.document.board, 500).expect("grid");
    let mut text = ContentLayer::new_text(
        "wrapped",
        "你好你好\n世界",
        TransformUm::rect(1_000, 1_000, 8_000, 9_000),
    );
    let atelier_core::ContentKind::Text(ref mut content) = text.kind else {
        unreachable!()
    };
    content.layout = atelier_core::TextLayout::FixedFrame;
    let operation = FabricationOperation {
        mapping_id: MappingId::new(),
        source_layer_id: text.id,
        source_name: text.name,
        combine: CombineMode::Add,
        transform: text.transform,
        primitive: match text.kind {
            atelier_core::ContentKind::Text(value) => FabricationPrimitive::Text(value),
            _ => unreachable!(),
        },
        clip_to_board: true,
    };
    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("font");
    let mask = rasterizer.rasterize(&operation, &grid).expect("rasterize");
    assert!(mask.active_pixel_count() > 0);
    for y in 0..grid.height_px {
        for x in 0..grid.width_px {
            if mask.get(x, y).expect("pixel") {
                assert!((2..18).contains(&x), "text escaped the fixed frame on x");
                assert!((2..20).contains(&y), "text escaped the fixed frame on y");
            }
        }
    }
}

#[test]
fn rounded_board_outline_clips_pixels_outside_its_corner() {
    let mut document = atelier_core::AtelierDocument::new_card("round", 40, 40);
    document.board = atelier_core::BoardOutline::RoundedRectangle {
        width_um: 40,
        height_um: 40,
        corner_radius_um: 20,
    };
    let mut bundle = ProjectBundle::new(document);
    let asset = bundle
        .embed_asset(
            "solid.png",
            "image/png",
            1,
            1,
            png(&[(0, 0, [0, 0, 0, 255])], 1, 1),
        )
        .expect("asset");
    let grid = RasterGrid::for_board(&bundle.document.board, 10).expect("grid");
    let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("font");
    let mask = rasterizer
        .rasterize(&operation(asset, TransformUm::rect(0, 0, 40, 40)), &grid)
        .expect("rasterize");
    assert!(!mask.get(0, 0).expect("corner clipped"));
    assert!(mask.get(1, 1).expect("inside rounded corner"));
}
