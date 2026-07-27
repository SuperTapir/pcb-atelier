use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, io::Cursor};
use thiserror::Error;

use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};

use crate::{AssetId, BitMask, CropRect, FabricationResolveError, PhysicalBoundsUm, TreatmentId};

pub const TREATMENT_ALGORITHM_VERSION: &str = "atelier-image-treatment-v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageTreatment {
    pub id: TreatmentId,
    pub asset_id: AssetId,
    pub production_mode: ImageProductionMode,
    pub recipe: TreatmentRecipe,
}

impl ImageTreatment {
    pub fn new(asset_id: AssetId, recipe: TreatmentRecipe) -> Self {
        Self {
            id: TreatmentId::new(),
            asset_id,
            production_mode: ImageProductionMode::MonochromeMask,
            recipe,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageProductionMode {
    #[default]
    MonochromeMask,
    ColorOriginal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TreatmentRecipe {
    pub algorithm_version: String,
    pub alpha_mode: AlphaMode,
    pub threshold: ThresholdMode,
    pub invert: bool,
    pub smoothing_radius_um: u32,
    pub despeckle_radius_um: u32,
    pub remove_islands_below_um2: u64,
    pub minimum_line_width_um: u32,
    pub thin_feature_policy: ThinFeaturePolicy,
    pub minimum_gap_um: u32,
    #[serde(deserialize_with = "crate::document::deserialize_required_option")]
    pub crop: Option<CropRect>,
}

impl TreatmentRecipe {
    pub fn standard_monochrome() -> Self {
        Self {
            algorithm_version: TREATMENT_ALGORITHM_VERSION.to_owned(),
            alpha_mode: AlphaMode::CompositeOnWhite,
            threshold: ThresholdMode::Manual { value: 128 },
            invert: false,
            smoothing_radius_um: 0,
            despeckle_radius_um: 0,
            remove_islands_below_um2: 0,
            minimum_line_width_um: 0,
            thin_feature_policy: ThinFeaturePolicy::Preserve,
            minimum_gap_um: 0,
            crop: None,
        }
    }

    pub fn fingerprint(&self) -> String {
        let encoded = serde_json::to_vec(self).expect("treatment recipe is serializable");
        Sha256::digest(encoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub fn validate(&self) -> Result<(), TreatmentRecipeValidationError> {
        if self.algorithm_version != TREATMENT_ALGORITHM_VERSION {
            return Err(TreatmentRecipeValidationError::UnsupportedAlgorithmVersion(
                self.algorithm_version.clone(),
            ));
        }
        if !valid_crop(self.crop.as_ref()) {
            return Err(TreatmentRecipeValidationError::InvalidCrop);
        }
        Ok(())
    }
}

impl Default for TreatmentRecipe {
    fn default() -> Self {
        Self::standard_monochrome()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlphaMode {
    CompositeOnWhite,
    AlphaAsCoverage,
    IgnoreAlpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum ThresholdMode {
    Otsu,
    Manual { value: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThinFeaturePolicy {
    Preserve,
    Thicken,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SamplingPurpose {
    InteractiveProxy,
    BoardPreview,
    FormalProduction,
}

impl SamplingPurpose {
    pub const fn default_pixel_pitch_um(self) -> u32 {
        match self {
            Self::InteractiveProxy => 250,
            Self::BoardPreview => 100,
            Self::FormalProduction => 25,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreatmentCompileRequest {
    pub physical_width_um: u32,
    pub physical_height_um: u32,
    pub pixel_pitch_um: u32,
    pub revision: u64,
    pub purpose: SamplingPurpose,
}

impl TreatmentCompileRequest {
    pub const fn for_purpose(
        physical_width_um: u32,
        physical_height_um: u32,
        revision: u64,
        purpose: SamplingPurpose,
    ) -> Self {
        Self {
            physical_width_um,
            physical_height_um,
            pixel_pitch_um: purpose.default_pixel_pitch_um(),
            revision,
            purpose,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskTopology {
    pub island_count: u32,
    pub hole_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TreatmentDiagnostic {
    RemovedSpeck { diameter_um: u32 },
    RemovedIsland { area_um2: u64 },
    FeatureBelowMinimumLineWidth { minimum_um: u32, measured_um: u32 },
    ThickenedThinFeature { minimum_um: u32, measured_um: u32 },
    RemovedThinFeature { minimum_um: u32, measured_um: u32 },
    GapBelowMinimum { minimum_um: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledImageTreatment {
    pub mask: BitMask,
    pub applied_threshold: u8,
    pub pixel_pitch_um: u32,
    pub bounds_um: PhysicalBoundsUm,
    pub recipe_fingerprint: String,
    pub revision: u64,
    pub purpose: SamplingPurpose,
    pub topology: MaskTopology,
    pub diagnostics: Vec<TreatmentDiagnostic>,
}

#[derive(Debug, Error)]
pub enum TreatmentCompileError {
    #[error("image treatment dimensions and pixel pitch must be positive")]
    InvalidDimensions,
    #[error("image treatment crop must be non-empty and contained in normalized source bounds")]
    InvalidCrop,
    #[error("unsupported image treatment algorithm version: {0}")]
    UnsupportedAlgorithmVersion(String),
    #[error("could not inspect image treatment source: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not decode image treatment source: {0}")]
    Decode(#[from] image::ImageError),
    #[error("could not allocate or access image treatment mask: {0}")]
    Mask(#[from] FabricationResolveError),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TreatmentRecipeValidationError {
    #[error("unsupported image treatment algorithm version: {0}")]
    UnsupportedAlgorithmVersion(String),
    #[error("image treatment crop must be non-empty and contained in normalized source bounds")]
    InvalidCrop,
}

pub fn compile_image_treatment(
    bytes: &[u8],
    recipe: &TreatmentRecipe,
    request: TreatmentCompileRequest,
) -> Result<CompiledImageTreatment, TreatmentCompileError> {
    if bytes.is_empty()
        || request.physical_width_um == 0
        || request.physical_height_um == 0
        || request.pixel_pitch_um == 0
    {
        return Err(TreatmentCompileError::InvalidDimensions);
    }
    recipe.validate().map_err(|error| match error {
        TreatmentRecipeValidationError::UnsupportedAlgorithmVersion(version) => {
            TreatmentCompileError::UnsupportedAlgorithmVersion(version)
        }
        TreatmentRecipeValidationError::InvalidCrop => TreatmentCompileError::InvalidCrop,
    })?;
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);

    let width_px = request.physical_width_um.div_ceil(request.pixel_pitch_um);
    let height_px = request.physical_height_um.div_ceil(request.pixel_pitch_um);
    let grayscale = sample_grayscale(&image, recipe, width_px, height_px);
    let threshold = match recipe.threshold {
        ThresholdMode::Otsu => otsu_threshold(&grayscale),
        ThresholdMode::Manual { value } => value,
    };
    let mut active = grayscale
        .into_iter()
        .map(|gray| {
            let ink = gray <= threshold;
            if recipe.invert { !ink } else { ink }
        })
        .collect::<Vec<_>>();

    if recipe.smoothing_radius_um > 0 {
        active = majority_smooth(
            &active,
            width_px,
            height_px,
            recipe.smoothing_radius_um.div_ceil(request.pixel_pitch_um),
        );
    }

    let mut diagnostics = remove_specks(
        &mut active,
        width_px,
        height_px,
        request.pixel_pitch_um,
        request.physical_width_um,
        request.physical_height_um,
        recipe.despeckle_radius_um,
    );
    diagnostics.extend(remove_small_islands(
        &mut active,
        width_px,
        height_px,
        request.pixel_pitch_um,
        request.physical_width_um,
        request.physical_height_um,
        recipe.remove_islands_below_um2,
    ));
    diagnostics.extend(apply_thin_feature_policy(
        &mut active,
        width_px,
        height_px,
        request.pixel_pitch_um,
        recipe.minimum_line_width_um,
        recipe.thin_feature_policy,
    ));
    if recipe.minimum_gap_um > 0
        && has_narrow_inactive_gap(
            &active,
            width_px,
            height_px,
            recipe.minimum_gap_um.div_ceil(request.pixel_pitch_um),
        )
    {
        diagnostics.push(TreatmentDiagnostic::GapBelowMinimum {
            minimum_um: recipe.minimum_gap_um,
        });
    }

    let topology = topology(&active, width_px, height_px);
    let mut mask = BitMask::new(width_px, height_px)?;
    for y in 0..height_px {
        for x in 0..width_px {
            mask.set(x, y, active[index(width_px, x, y)])?;
        }
    }
    Ok(CompiledImageTreatment {
        mask,
        applied_threshold: threshold,
        pixel_pitch_um: request.pixel_pitch_um,
        bounds_um: PhysicalBoundsUm {
            min_x_um: 0,
            min_y_um: 0,
            max_x_um: i64::from(request.physical_width_um),
            max_y_um: i64::from(request.physical_height_um),
        },
        recipe_fingerprint: recipe.fingerprint(),
        revision: request.revision,
        purpose: request.purpose,
        topology,
        diagnostics,
    })
}

pub fn treatment_cache_key(
    asset_sha256: &str,
    recipe: &TreatmentRecipe,
    physical_width_um: u32,
    physical_height_um: u32,
    pixel_pitch_um: u32,
) -> String {
    let mut digest = Sha256::new();
    digest.update(TREATMENT_ALGORITHM_VERSION.as_bytes());
    digest.update(asset_sha256.as_bytes());
    digest.update(recipe.fingerprint().as_bytes());
    digest.update(physical_width_um.to_le_bytes());
    digest.update(physical_height_um.to_le_bytes());
    digest.update(pixel_pitch_um.to_le_bytes());
    format!("{:x}", digest.finalize())
}

fn sample_grayscale(
    image: &DynamicImage,
    recipe: &TreatmentRecipe,
    width_px: u32,
    height_px: u32,
) -> Vec<u8> {
    let crop = recipe.crop.clone().unwrap_or(CropRect {
        x_millionths: 0,
        y_millionths: 0,
        width_millionths: 1_000_000,
        height_millionths: 1_000_000,
    });
    let mut result = Vec::with_capacity((u64::from(width_px) * u64::from(height_px)) as usize);
    for y in 0..height_px {
        for x in 0..width_px {
            let u = (f64::from(x) + 0.5) / f64::from(width_px);
            let v = (f64::from(y) + 0.5) / f64::from(height_px);
            let source_u =
                (f64::from(crop.x_millionths) + u * f64::from(crop.width_millionths)) / 1_000_000.0;
            let source_v = (f64::from(crop.y_millionths) + v * f64::from(crop.height_millionths))
                / 1_000_000.0;
            let source_x = (source_u * f64::from(image.width()))
                .floor()
                .clamp(0.0, f64::from(image.width().saturating_sub(1)))
                as u32;
            let source_y = (source_v * f64::from(image.height()))
                .floor()
                .clamp(0.0, f64::from(image.height().saturating_sub(1)))
                as u32;
            let pixel = image.get_pixel(source_x, source_y).0;
            let luminance =
                (299 * u32::from(pixel[0]) + 587 * u32::from(pixel[1]) + 114 * u32::from(pixel[2]))
                    / 1_000;
            let alpha = u32::from(pixel[3]);
            let gray = match recipe.alpha_mode {
                AlphaMode::CompositeOnWhite => (luminance * alpha + 255 * (255 - alpha)) / 255,
                AlphaMode::AlphaAsCoverage => 255 - alpha,
                AlphaMode::IgnoreAlpha => luminance,
            };
            result.push(gray as u8);
        }
    }
    result
}

fn valid_crop(crop: Option<&CropRect>) -> bool {
    let Some(crop) = crop else {
        return true;
    };
    let max = 1_000_000_u32;
    crop.width_millionths > 0
        && crop.height_millionths > 0
        && crop
            .x_millionths
            .checked_add(crop.width_millionths)
            .is_some_and(|right| right <= max)
        && crop
            .y_millionths
            .checked_add(crop.height_millionths)
            .is_some_and(|bottom| bottom <= max)
}

fn otsu_threshold(values: &[u8]) -> u8 {
    let mut histogram = [0_u64; 256];
    for value in values {
        histogram[usize::from(*value)] += 1;
    }
    let total = values.len() as u64;
    let total_sum = histogram
        .iter()
        .enumerate()
        .map(|(value, count)| value as u64 * count)
        .sum::<u64>();
    let mut background_count = 0_u64;
    let mut background_sum = 0_u64;
    let mut best_variance = -1.0_f64;
    let mut best_threshold = 0_u8;
    for (threshold, count) in histogram.iter().enumerate() {
        background_count += count;
        if background_count == 0 {
            continue;
        }
        let foreground_count = total - background_count;
        if foreground_count == 0 {
            break;
        }
        background_sum += threshold as u64 * count;
        let background_mean = background_sum as f64 / background_count as f64;
        let foreground_mean = (total_sum - background_sum) as f64 / foreground_count as f64;
        let variance = background_count as f64
            * foreground_count as f64
            * (background_mean - foreground_mean).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best_threshold = threshold as u8;
        }
    }
    best_threshold
}

fn majority_smooth(source: &[bool], width: u32, height: u32, radius: u32) -> Vec<bool> {
    if radius == 0 {
        return source.to_vec();
    }
    let mut result = source.to_vec();
    for y in 0..height {
        for x in 0..width {
            let mut active = 0_u32;
            let mut total = 0_u32;
            let y_min = y.saturating_sub(radius);
            let y_max = y.saturating_add(radius).min(height - 1);
            let x_min = x.saturating_sub(radius);
            let x_max = x.saturating_add(radius).min(width - 1);
            for sample_y in y_min..=y_max {
                for sample_x in x_min..=x_max {
                    if source[index(width, sample_x, sample_y)] {
                        active += 1;
                    }
                    total += 1;
                }
            }
            result[index(width, x, y)] = active * 2 >= total;
        }
    }
    result
}

fn remove_small_islands(
    active: &mut [bool],
    width: u32,
    height: u32,
    pixel_pitch_um: u32,
    physical_width_um: u32,
    physical_height_um: u32,
    minimum_area_um2: u64,
) -> Vec<TreatmentDiagnostic> {
    if minimum_area_um2 == 0 {
        return Vec::new();
    }
    let mut visited = vec![false; active.len()];
    let mut diagnostics = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let start = index(width, x, y);
            if visited[start] || !active[start] {
                continue;
            }
            let component = flood_component(active, width, height, x, y, true, &mut visited);
            let area_um2 = component
                .iter()
                .map(|point| {
                    let point = *point as u32;
                    let px = point % width;
                    let py = point / width;
                    let pixel_width = physical_width_um
                        .saturating_sub(px.saturating_mul(pixel_pitch_um))
                        .min(pixel_pitch_um);
                    let pixel_height = physical_height_um
                        .saturating_sub(py.saturating_mul(pixel_pitch_um))
                        .min(pixel_pitch_um);
                    u64::from(pixel_width) * u64::from(pixel_height)
                })
                .sum();
            if area_um2 < minimum_area_um2 {
                for point in component {
                    active[point] = false;
                }
                diagnostics.push(TreatmentDiagnostic::RemovedIsland { area_um2 });
            }
        }
    }
    diagnostics
}

fn remove_specks(
    active: &mut [bool],
    width: u32,
    height: u32,
    pixel_pitch_um: u32,
    physical_width_um: u32,
    physical_height_um: u32,
    maximum_diameter_um: u32,
) -> Vec<TreatmentDiagnostic> {
    if maximum_diameter_um == 0 {
        return Vec::new();
    }
    let mut visited = vec![false; active.len()];
    let mut diagnostics = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let start = index(width, x, y);
            if visited[start] || !active[start] {
                continue;
            }
            let component = flood_component(active, width, height, x, y, true, &mut visited);
            let mut min_x = width;
            let mut min_y = height;
            let mut max_x = 0;
            let mut max_y = 0;
            for point in &component {
                let point = *point as u32;
                let px = point % width;
                let py = point / width;
                min_x = min_x.min(px);
                min_y = min_y.min(py);
                max_x = max_x.max(px);
                max_y = max_y.max(py);
            }
            let component_width_um = physical_width_um
                .min((max_x + 1).saturating_mul(pixel_pitch_um))
                .saturating_sub(min_x.saturating_mul(pixel_pitch_um));
            let component_height_um = physical_height_um
                .min((max_y + 1).saturating_mul(pixel_pitch_um))
                .saturating_sub(min_y.saturating_mul(pixel_pitch_um));
            let diameter_um = component_width_um.max(component_height_um);
            if diameter_um <= maximum_diameter_um {
                for point in component {
                    active[point] = false;
                }
                diagnostics.push(TreatmentDiagnostic::RemovedSpeck { diameter_um });
            }
        }
    }
    diagnostics
}

fn apply_thin_feature_policy(
    active: &mut [bool],
    width: u32,
    height: u32,
    pixel_pitch_um: u32,
    minimum_width_um: u32,
    policy: ThinFeaturePolicy,
) -> Vec<TreatmentDiagnostic> {
    if minimum_width_um == 0 {
        return Vec::new();
    }
    let minimum_pixels = minimum_width_um.div_ceil(pixel_pitch_um).max(1);
    let risky = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|(x, y)| {
            active[index(width, *x, *y)]
                && local_feature_width(active, width, height, *x, *y) < minimum_pixels
        })
        .collect::<Vec<_>>();
    if risky.is_empty() {
        return Vec::new();
    }
    let measured_pixels = risky
        .iter()
        .map(|(x, y)| local_feature_width(active, width, height, *x, *y))
        .min()
        .unwrap_or(1);
    let measured_um = measured_pixels.saturating_mul(pixel_pitch_um);
    let diagnostic = match policy {
        ThinFeaturePolicy::Preserve => TreatmentDiagnostic::FeatureBelowMinimumLineWidth {
            minimum_um: minimum_width_um,
            measured_um,
        },
        ThinFeaturePolicy::Thicken => {
            let radius = minimum_pixels.saturating_sub(measured_pixels).div_ceil(2);
            let source = active.to_vec();
            for (x, y) in risky {
                for target_y in y.saturating_sub(radius)..=y.saturating_add(radius).min(height - 1)
                {
                    for target_x in
                        x.saturating_sub(radius)..=x.saturating_add(radius).min(width - 1)
                    {
                        if source[index(width, x, y)] {
                            active[index(width, target_x, target_y)] = true;
                        }
                    }
                }
            }
            TreatmentDiagnostic::ThickenedThinFeature {
                minimum_um: minimum_width_um,
                measured_um,
            }
        }
        ThinFeaturePolicy::Remove => {
            for (x, y) in risky {
                active[index(width, x, y)] = false;
            }
            TreatmentDiagnostic::RemovedThinFeature {
                minimum_um: minimum_width_um,
                measured_um,
            }
        }
    };
    vec![diagnostic]
}

fn local_feature_width(active: &[bool], width: u32, height: u32, x: u32, y: u32) -> u32 {
    let mut left = x;
    while left > 0 && active[index(width, left - 1, y)] {
        left -= 1;
    }
    let mut right = x;
    while right + 1 < width && active[index(width, right + 1, y)] {
        right += 1;
    }
    let mut top = y;
    while top > 0 && active[index(width, x, top - 1)] {
        top -= 1;
    }
    let mut bottom = y;
    while bottom + 1 < height && active[index(width, x, bottom + 1)] {
        bottom += 1;
    }
    let horizontal = right - left + 1;
    let vertical = bottom - top + 1;
    horizontal.min(vertical)
}

fn topology(active: &[bool], width: u32, height: u32) -> MaskTopology {
    let island_count = component_count(active, width, height, true, false);
    let hole_count = component_count(active, width, height, false, true);
    MaskTopology {
        island_count,
        hole_count,
    }
}

fn component_count(
    active: &[bool],
    width: u32,
    height: u32,
    target: bool,
    exclude_border: bool,
) -> u32 {
    let mut visited = vec![false; active.len()];
    let mut count = 0;
    for y in 0..height {
        for x in 0..width {
            let start = index(width, x, y);
            if visited[start] || active[start] != target {
                continue;
            }
            let component = flood_component(active, width, height, x, y, target, &mut visited);
            if !exclude_border
                || !component.iter().any(|point| {
                    let point = *point as u32;
                    let px = point % width;
                    let py = point / width;
                    px == 0 || py == 0 || px + 1 == width || py + 1 == height
                })
            {
                count += 1;
            }
        }
    }
    count
}

fn flood_component(
    active: &[bool],
    width: u32,
    height: u32,
    start_x: u32,
    start_y: u32,
    target: bool,
    visited: &mut [bool],
) -> Vec<usize> {
    let mut queue = VecDeque::from([(start_x, start_y)]);
    let mut component = Vec::new();
    visited[index(width, start_x, start_y)] = true;
    while let Some((x, y)) = queue.pop_front() {
        component.push(index(width, x, y));
        for (next_x, next_y) in neighbors(x, y, width, height) {
            let next = index(width, next_x, next_y);
            if !visited[next] && active[next] == target {
                visited[next] = true;
                queue.push_back((next_x, next_y));
            }
        }
    }
    component
}

fn neighbors(x: u32, y: u32, width: u32, height: u32) -> impl Iterator<Item = (u32, u32)> {
    [
        x.checked_sub(1).map(|next| (next, y)),
        (x + 1 < width).then_some((x + 1, y)),
        y.checked_sub(1).map(|next| (x, next)),
        (y + 1 < height).then_some((x, y + 1)),
    ]
    .into_iter()
    .flatten()
}

fn has_narrow_inactive_gap(active: &[bool], width: u32, height: u32, minimum_pixels: u32) -> bool {
    minimum_pixels > 1
        && (has_bounded_gap_runs(active, width, height, minimum_pixels, true)
            || has_bounded_gap_runs(active, height, width, minimum_pixels, false))
}

fn has_bounded_gap_runs(
    active: &[bool],
    primary_limit: u32,
    secondary_limit: u32,
    minimum_pixels: u32,
    horizontal: bool,
) -> bool {
    (0..secondary_limit).any(|secondary| {
        let mut position = 0;
        while position < primary_limit {
            let at = |primary| {
                let (x, y) = if horizontal {
                    (primary, secondary)
                } else {
                    (secondary, primary)
                };
                active[index(
                    if horizontal {
                        primary_limit
                    } else {
                        secondary_limit
                    },
                    x,
                    y,
                )]
            };
            if at(position) {
                position += 1;
                continue;
            }
            let start = position;
            while position < primary_limit && !at(position) {
                position += 1;
            }
            if start > 0
                && position < primary_limit
                && position - start < minimum_pixels
                && at(start - 1)
                && at(position)
            {
                return true;
            }
        }
        false
    })
}

fn index(width: u32, x: u32, y: u32) -> usize {
    (u64::from(y) * u64::from(width) + u64::from(x)) as usize
}
