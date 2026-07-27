//! Project-bundle rasterizer for resolved fabrication masks.
//!
//! Images come only from the supplied `ProjectBundle`. Text uses the selected
//! system font when it is available and falls back to the embedded Noto Sans
//! CJK SC bytes shipped with this application.

use std::{
    collections::{BTreeSet, HashMap},
    sync::OnceLock,
};

use fontdb::{Database, Family, Query};
use fontdue::Font;
use image::{DynamicImage, GenericImageView};
use sha2::{Digest, Sha256};

use crate::{
    BitMask, BoardOutline, CropRect, FabricationOperation, FabricationPrimitive,
    FabricationRasterizer, ProjectBundle, RasterGrid, SamplingPurpose, TextContent, TransformUm,
    TreatmentCompileRequest, TreatmentRecipe, compile_image_treatment,
};

pub const ALPHA_THRESHOLD: u8 = 128;
const RASTERIZER_VERSION: &str = "atelier-bundle-rasterizer-v3";
const NOTO_SANS_CJK_SC: &[u8] = include_bytes!("../../../assets/fonts/NotoSansSC-Regular.otf");
static SYSTEM_FONT_DATABASE: OnceLock<Database> = OnceLock::new();

fn system_font_database() -> &'static Database {
    SYSTEM_FONT_DATABASE.get_or_init(|| {
        let mut database = Database::new();
        database.load_system_fonts();
        database
    })
}

pub struct ProjectBundleRasterizer<'a> {
    bundle: &'a ProjectBundle,
    fonts: HashMap<String, Font>,
    font_fingerprint: String,
}

impl<'a> ProjectBundleRasterizer<'a> {
    pub fn new(bundle: &'a ProjectBundle) -> Result<Self, String> {
        let fallback = Font::from_bytes(NOTO_SANS_CJK_SC, fontdue::FontSettings::default())
            .map_err(|error| format!("embedded Noto Sans CJK SC font is invalid: {error}"))?;
        let mut fonts = HashMap::from([("sans-serif".to_owned(), fallback)]);
        let mut fingerprints = vec![format!(
            "sans-serif:{}",
            hex(&Sha256::digest(NOTO_SANS_CJK_SC))
        )];
        let database = system_font_database();
        let families = bundle
            .document
            .front
            .layers
            .iter()
            .chain(&bundle.document.back.layers)
            .filter_map(|layer| match &layer.kind {
                crate::ContentKind::Text(text) if text.font_family != "sans-serif" => {
                    Some(text.font_family.clone())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        for family in families {
            let query = Query {
                families: &[Family::Name(&family)],
                ..Query::default()
            };
            let Some(id) = database.query(&query) else {
                continue;
            };
            let loaded = database.with_face_data(id, |bytes, face_index| {
                let settings = fontdue::FontSettings {
                    collection_index: face_index,
                    ..fontdue::FontSettings::default()
                };
                Font::from_bytes(bytes, settings).map(|font| (font, hex(&Sha256::digest(bytes))))
            });
            if let Some(Ok((font, fingerprint))) = loaded {
                fingerprints.push(format!("{family}:{fingerprint}"));
                fonts.insert(family, font);
            }
        }
        fingerprints.sort();
        let font_fingerprint = format!("{RASTERIZER_VERSION}:{}", fingerprints.join("|"));
        Ok(Self {
            bundle,
            fonts,
            font_fingerprint,
        })
    }

    pub fn font_fingerprint(&self) -> &str {
        &self.font_fingerprint
    }

    fn rasterize_image(
        &self,
        asset_id: crate::AssetId,
        crop: Option<&CropRect>,
        treatment: Option<&TreatmentRecipe>,
        transform: TransformUm,
        grid: &RasterGrid,
    ) -> Result<BitMask, String> {
        let bytes = self
            .bundle
            .asset_bytes(asset_id)
            .ok_or_else(|| format!("missing image asset {asset_id}"))?;
        let treated = treatment
            .map(|recipe| {
                let mut effective_recipe = recipe.clone();
                if effective_recipe.crop.is_none() {
                    effective_recipe.crop = crop.cloned();
                }
                compile_image_treatment(
                    bytes,
                    &effective_recipe,
                    TreatmentCompileRequest {
                        physical_width_um: transform.width_um,
                        physical_height_um: transform.height_um,
                        pixel_pitch_um: grid.pixel_pitch_um,
                        revision: 0,
                        purpose: SamplingPurpose::FormalProduction,
                    },
                )
                .map_err(|error| format!("treat image asset {asset_id}: {error}"))
            })
            .transpose()?;
        let image = treatment
            .is_none()
            .then(|| {
                image::load_from_memory(bytes)
                    .map_err(|error| format!("decode image asset {asset_id}: {error}"))
            })
            .transpose()?;
        let mut mask =
            BitMask::new(grid.width_px, grid.height_px).map_err(|error| error.to_string())?;
        for y in 0..grid.height_px {
            for x in 0..grid.width_px {
                if !inside_outline(&self.bundle.document.board, grid, x, y) {
                    continue;
                }
                let Some((u, v)) = inverse_transform(grid, x, y, transform) else {
                    continue;
                };
                let is_ink = if let Some(treated) = &treated {
                    let source_x = (u * f64::from(treated.mask.width_px()))
                        .floor()
                        .clamp(0.0, f64::from(treated.mask.width_px().saturating_sub(1)))
                        as u32;
                    let source_y = (v * f64::from(treated.mask.height_px()))
                        .floor()
                        .clamp(0.0, f64::from(treated.mask.height_px().saturating_sub(1)))
                        as u32;
                    treated
                        .mask
                        .get(source_x, source_y)
                        .map_err(|error| error.to_string())?
                } else {
                    let image = image.as_ref().expect("legacy image was decoded");
                    let (source_x, source_y) = crop_sample(image, crop, u, v);
                    is_production_ink(image.get_pixel(source_x, source_y).0)
                };
                if is_ink {
                    mask.set(x, y, true).map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(mask)
    }

    fn rasterize_text(
        &self,
        text: &TextContent,
        transform: TransformUm,
        grid: &RasterGrid,
    ) -> Result<BitMask, String> {
        let font = self
            .fonts
            .get(&text.font_family)
            .or_else(|| self.fonts.get("sans-serif"))
            .expect("embedded fallback font must exist");
        let mut mask =
            BitMask::new(grid.width_px, grid.height_px).map_err(|error| error.to_string())?;
        let pixels_per_em = (text.font_size_um as f32 / grid.pixel_pitch_um as f32).max(1.0);
        let mut pen_x = 0_i32;
        let mut baseline = pixels_per_em.ceil() as i32;
        let line_height = (pixels_per_em * 1.2).ceil() as i32;
        let frame_width_px = (transform.width_um / grid.pixel_pitch_um) as i32;
        let frame_height_px = (transform.height_um / grid.pixel_pitch_um) as i32;
        for character in text.text.chars() {
            if character == '\n' {
                pen_x = 0;
                baseline += line_height;
                continue;
            }
            let (metrics, bitmap) = font.rasterize(character, pixels_per_em);
            let advance = metrics.advance_width.ceil() as i32;
            if matches!(text.layout, crate::TextLayout::FixedFrame)
                && pen_x > 0
                && pen_x + advance > frame_width_px
            {
                pen_x = 0;
                baseline += line_height;
            }
            if matches!(text.layout, crate::TextLayout::FixedFrame)
                && baseline - pixels_per_em.ceil() as i32 >= frame_height_px
            {
                break;
            }
            for glyph_y in 0..metrics.height {
                for glyph_x in 0..metrics.width {
                    if bitmap[glyph_y * metrics.width + glyph_x] < ALPHA_THRESHOLD {
                        continue;
                    }
                    let local_x_px = pen_x + metrics.xmin + glyph_x as i32;
                    let local_y_px = baseline + metrics.ymin + glyph_y as i32;
                    if matches!(text.layout, crate::TextLayout::FixedFrame)
                        && (local_x_px < 0
                            || local_y_px < 0
                            || local_x_px >= frame_width_px
                            || local_y_px >= frame_height_px)
                    {
                        continue;
                    }
                    let local_x_um = i64::from(local_x_px) * i64::from(grid.pixel_pitch_um);
                    let local_y_um = i64::from(local_y_px) * i64::from(grid.pixel_pitch_um);
                    if let Some((x, y)) = forward_transform(grid, local_x_um, local_y_um, transform)
                        && inside_outline(&self.bundle.document.board, grid, x, y)
                    {
                        mask.set(x, y, true).map_err(|error| error.to_string())?;
                    }
                }
            }
            pen_x += advance;
        }
        Ok(mask)
    }
}

pub fn system_font_families() -> Vec<String> {
    system_font_database()
        .faces()
        .flat_map(|face| face.families.iter().map(|(family, _)| family.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

impl FabricationRasterizer for ProjectBundleRasterizer<'_> {
    fn fingerprint(&self) -> String {
        self.font_fingerprint.clone()
    }

    fn rasterize(
        &mut self,
        operation: &FabricationOperation,
        grid: &RasterGrid,
    ) -> Result<BitMask, String> {
        match &operation.primitive {
            FabricationPrimitive::Image {
                asset_id,
                crop,
                treatment,
            } => self.rasterize_image(
                *asset_id,
                crop.as_ref(),
                treatment.as_ref(),
                operation.transform,
                grid,
            ),
            FabricationPrimitive::Text(text) => {
                self.rasterize_text(text, operation.transform, grid)
            }
            FabricationPrimitive::BoardFill {
                outline,
                edge_clearance_um,
            } => rasterize_board_fill(outline, *edge_clearance_um, grid),
        }
    }
}

fn rasterize_board_fill(
    outline: &BoardOutline,
    edge_clearance_um: u32,
    grid: &RasterGrid,
) -> Result<BitMask, String> {
    let mut mask =
        BitMask::new(grid.width_px, grid.height_px).map_err(|error| error.to_string())?;
    for y in 0..grid.height_px {
        for x in 0..grid.width_px {
            if inside_inset_outline(outline, edge_clearance_um, grid, x, y) {
                mask.set(x, y, true).map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(mask)
}

fn inverse_transform(
    grid: &RasterGrid,
    x: u32,
    y: u32,
    transform: TransformUm,
) -> Option<(f64, f64)> {
    if transform.width_um == 0 || transform.height_um == 0 {
        return None;
    }
    let point_x = grid.origin_x_um as f64 + (x as f64 + 0.5) * grid.pixel_pitch_um as f64;
    let point_y = grid.origin_y_um as f64 + (y as f64 + 0.5) * grid.pixel_pitch_um as f64;
    let center_x = transform.x_um as f64 + transform.width_um as f64 / 2.0;
    let center_y = transform.y_um as f64 + transform.height_um as f64 / 2.0;
    let radians = -(transform.rotation_mdeg as f64 / 1000.0).to_radians();
    let dx = point_x - center_x;
    let dy = point_y - center_y;
    let mut local_x = dx * radians.cos() - dy * radians.sin() + transform.width_um as f64 / 2.0;
    let mut local_y = dx * radians.sin() + dy * radians.cos() + transform.height_um as f64 / 2.0;
    if transform.flip_x {
        local_x = transform.width_um as f64 - local_x;
    }
    if transform.flip_y {
        local_y = transform.height_um as f64 - local_y;
    }
    if !(0.0..transform.width_um as f64).contains(&local_x)
        || !(0.0..transform.height_um as f64).contains(&local_y)
    {
        return None;
    }
    Some((
        local_x / transform.width_um as f64,
        local_y / transform.height_um as f64,
    ))
}

fn forward_transform(
    grid: &RasterGrid,
    local_x_um: i64,
    local_y_um: i64,
    transform: TransformUm,
) -> Option<(u32, u32)> {
    let mut local_x = local_x_um as f64;
    let mut local_y = local_y_um as f64;
    if transform.flip_x {
        local_x = transform.width_um as f64 - local_x;
    }
    if transform.flip_y {
        local_y = transform.height_um as f64 - local_y;
    }
    let radians = (transform.rotation_mdeg as f64 / 1000.0).to_radians();
    let centered_x = local_x - transform.width_um as f64 / 2.0;
    let centered_y = local_y - transform.height_um as f64 / 2.0;
    let board_x =
        transform.x_um as f64 + transform.width_um as f64 / 2.0 + centered_x * radians.cos()
            - centered_y * radians.sin();
    let board_y = transform.y_um as f64
        + transform.height_um as f64 / 2.0
        + centered_x * radians.sin()
        + centered_y * radians.cos();
    let x = ((board_x - grid.origin_x_um as f64) / grid.pixel_pitch_um as f64).floor() as i64;
    let y = ((board_y - grid.origin_y_um as f64) / grid.pixel_pitch_um as f64).floor() as i64;
    (x >= 0 && y >= 0 && x < i64::from(grid.width_px) && y < i64::from(grid.height_px))
        .then_some((x as u32, y as u32))
}

fn crop_sample(image: &DynamicImage, crop: Option<&CropRect>, u: f64, v: f64) -> (u32, u32) {
    let crop = crop.cloned().unwrap_or(CropRect {
        x_millionths: 0,
        y_millionths: 0,
        width_millionths: 1_000_000,
        height_millionths: 1_000_000,
    });
    let source_u = (crop.x_millionths as f64 + u * crop.width_millionths as f64) / 1_000_000.0;
    let source_v = (crop.y_millionths as f64 + v * crop.height_millionths as f64) / 1_000_000.0;
    let x = (source_u * image.width() as f64)
        .floor()
        .clamp(0.0, image.width().saturating_sub(1) as f64) as u32;
    let y = (source_v * image.height() as f64)
        .floor()
        .clamp(0.0, image.height().saturating_sub(1) as f64) as u32;
    (x, y)
}

fn is_production_ink([red, green, blue, alpha]: [u8; 4]) -> bool {
    // Interpret image darkness as deposited production material and alpha as
    // coverage. This keeps transparent black artwork useful while preventing
    // an opaque white JPEG background from becoming a solid copper rectangle.
    let luminance = (299 * u32::from(red) + 587 * u32::from(green) + 114 * u32::from(blue)) / 1_000;
    let coverage = u32::from(alpha) * (255 - luminance) / 255;
    coverage >= u32::from(ALPHA_THRESHOLD)
}

fn inside_outline(outline: &BoardOutline, grid: &RasterGrid, x: u32, y: u32) -> bool {
    let px = (x as f64 + 0.5) * grid.pixel_pitch_um as f64;
    let py = (y as f64 + 0.5) * grid.pixel_pitch_um as f64;
    match outline {
        BoardOutline::Rectangle {
            width_um,
            height_um,
        } => px < *width_um as f64 && py < *height_um as f64,
        BoardOutline::RoundedRectangle {
            width_um,
            height_um,
            corner_radius_um,
        } => {
            let (width, height, radius) = (
                *width_um as f64,
                *height_um as f64,
                *corner_radius_um as f64,
            );
            if px >= width || py >= height {
                return false;
            }
            let nearest_x = px.clamp(radius, width - radius);
            let nearest_y = py.clamp(radius, height - radius);
            (px - nearest_x).powi(2) + (py - nearest_y).powi(2) <= radius.powi(2)
        }
    }
}

fn inside_inset_outline(
    outline: &BoardOutline,
    edge_clearance_um: u32,
    grid: &RasterGrid,
    x: u32,
    y: u32,
) -> bool {
    let clearance = edge_clearance_um as f64;
    let px = (x as f64 + 0.5) * grid.pixel_pitch_um as f64;
    let py = (y as f64 + 0.5) * grid.pixel_pitch_um as f64;
    let width = outline.width_um() as f64;
    let height = outline.height_um() as f64;
    if px < clearance || py < clearance || px >= width - clearance || py >= height - clearance {
        return false;
    }
    match outline {
        BoardOutline::Rectangle { .. } => true,
        BoardOutline::RoundedRectangle {
            corner_radius_um, ..
        } => {
            let radius = (*corner_radius_um as f64 - clearance).max(0.0);
            if radius == 0.0 {
                return true;
            }
            let nearest_x = px.clamp(clearance + radius, width - clearance - radius);
            let nearest_y = py.clamp(clearance + radius, height - clearance - radius);
            (px - nearest_x).powi(2) + (py - nearest_y).powi(2) <= radius.powi(2)
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
