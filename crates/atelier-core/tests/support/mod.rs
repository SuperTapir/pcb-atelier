use atelier_core::{
    AtelierDocument, CardSide, CombineMode, ContentLayer, FaceProductionLayer, LayerId,
    MechanicalFeature, ProductionMapping, ProductionTarget, ProjectBundle, TransformUm,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;

pub struct GoldenCard {
    pub bundle: ProjectBundle,
    pub front_text_id: LayerId,
    pub back_text_id: LayerId,
}

pub fn asymmetric_golden_card() -> GoldenCard {
    let document = AtelierDocument::new_card("非对称黄金卡片", 64_000, 100_000);
    let mut bundle = ProjectBundle::new(document);
    let front_asset_id = bundle
        .embed_asset("front.png", "image/png", 4, 4, asymmetric_png(true))
        .expect("embed golden front asset");
    let back_asset_id = bundle
        .embed_asset("back.png", "image/png", 4, 4, asymmetric_png(false))
        .expect("embed golden back asset");

    let front_image = ContentLayer::new_image(
        "正面图片",
        front_asset_id,
        TransformUm::rect(-10_000, 9_000, 31_000, 44_000),
    );
    let front_image_id = front_image.id;
    let front_text = ContentLayer::new_text(
        "正面标记",
        "F",
        TransformUm::rect(43_000, 6_000, 8_000, 11_000),
    );
    let front_text_id = front_text.id;
    let back_image = ContentLayer::new_image(
        "背面图片",
        back_asset_id,
        TransformUm::rect(26_000, 38_000, 29_000, 47_000),
    );
    let back_image_id = back_image.id;
    let back_text = ContentLayer::new_text(
        "背面标记",
        "B",
        TransformUm::rect(7_000, 72_000, 9_000, 12_000),
    );
    let back_text_id = back_text.id;
    bundle.document.front.layers = vec![front_image, front_text];
    bundle.document.back.layers = vec![back_image, back_text];
    bundle.document.mappings = vec![
        mapping(front_image_id, CardSide::Front, FaceProductionLayer::Copper),
        mapping(
            front_image_id,
            CardSide::Front,
            FaceProductionLayer::SolderMaskOpen,
        ),
        mapping(
            front_text_id,
            CardSide::Front,
            FaceProductionLayer::Silkscreen,
        ),
        mapping(back_image_id, CardSide::Back, FaceProductionLayer::Copper),
        mapping(
            back_image_id,
            CardSide::Back,
            FaceProductionLayer::SolderMaskOpen,
        ),
        mapping(
            back_text_id,
            CardSide::Back,
            FaceProductionLayer::Silkscreen,
        ),
    ];
    bundle
        .document
        .mechanical_features
        .push(MechanicalFeature::NpthRound {
            center_x_um: 53_000,
            center_y_um: 14_000,
            diameter_um: 3_000,
        });

    GoldenCard {
        bundle,
        front_text_id,
        back_text_id,
    }
}

fn asymmetric_png(front: bool) -> Vec<u8> {
    let mut image = RgbaImage::new(4, 4);
    if front {
        image.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        image.put_pixel(3, 2, Rgba([0, 0, 0, 255]));
    } else {
        image.put_pixel(3, 0, Rgba([0, 0, 0, 255]));
        image.put_pixel(1, 3, Rgba([0, 0, 0, 255]));
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encode golden PNG");
    bytes.into_inner()
}

fn mapping(
    source_layer_id: LayerId,
    side: CardSide,
    layer: FaceProductionLayer,
) -> ProductionMapping {
    ProductionMapping::new(
        source_layer_id,
        ProductionTarget::new(side, layer),
        CombineMode::Add,
    )
}
