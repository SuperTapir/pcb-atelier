use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    AssetId, AtelierDocument, BitMask, ContentKind, LayerId, LayerPolarity,
    ManufacturerProfileSnapshot, MappingId, MaskTopology, PhysicalBoundsUm, ProductionTarget,
    RasterGrid, ResolvedFabricationBoard, TreatmentId,
};

pub const PRODUCTION_TRACE_FORMAT: &str = "atelier-production-trace-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionTraceReport {
    pub format: String,
    pub revision: u64,
    pub coordinate_space: ProductionCoordinateSpace,
    pub manufacturer_profile: ManufacturerProfileSnapshot,
    pub manufacturer_profile_fingerprint: String,
    pub fabrication_input_sha256: String,
    pub fabrication_output_sha256: String,
    pub layers: Vec<ProductionLayerTrace>,
    pub operations: Vec<ProductionOperationTrace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProductionCoordinateSpace {
    BoardPhysicalUpright,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionLayerTrace {
    pub target: ProductionTarget,
    pub polarity: LayerPolarity,
    pub composite_sha256: String,
    pub bounds_um: Option<PhysicalBoundsUm>,
    pub topology: MaskTopology,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionOperationTrace {
    pub mapping_id: MappingId,
    pub source_layer_id: LayerId,
    pub target: ProductionTarget,
    pub mask_sha256: String,
    pub asset_id: Option<AssetId>,
    pub asset_sha256: Option<String>,
    pub asset_media_type: Option<String>,
    pub treatment_id: Option<TreatmentId>,
    pub image_production_mode: Option<crate::ImageProductionMode>,
    pub algorithm_version: Option<String>,
    pub recipe_fingerprint: Option<String>,
}

pub fn build_production_trace(
    revision: u64,
    document: &AtelierDocument,
    resolved: &ResolvedFabricationBoard,
) -> ProductionTraceReport {
    let layers = resolved
        .layers
        .iter()
        .map(|layer| ProductionLayerTrace {
            target: layer.target,
            polarity: layer.polarity,
            composite_sha256: layer.composite_sha256.clone(),
            bounds_um: active_physical_bounds(&layer.composite, resolved.grid),
            topology: mask_topology(&layer.composite),
        })
        .collect();
    let operations = resolved
        .layers
        .iter()
        .flat_map(|layer| {
            layer.operations.iter().filter_map(move |operation| {
                let mapping = document
                    .mappings
                    .iter()
                    .find(|mapping| mapping.id == operation.mapping_id)?;
                let source = document
                    .front
                    .layers
                    .iter()
                    .chain(&document.back.layers)
                    .find(|source| source.id == operation.source_layer_id)?;
                let (asset_id, asset_sha256, asset_media_type) = match &source.kind {
                    ContentKind::Image(image) => {
                        let asset = document
                            .assets
                            .iter()
                            .find(|asset| asset.id == image.asset_id);
                        (
                            Some(image.asset_id),
                            asset.map(|asset| asset.sha256.clone()),
                            asset.map(|asset| asset.media_type.clone()),
                        )
                    }
                    _ => (None, None, None),
                };
                let treatment = mapping.treatment_id.and_then(|treatment_id| {
                    document
                        .image_treatments
                        .iter()
                        .find(|treatment| treatment.id == treatment_id)
                });
                Some(ProductionOperationTrace {
                    mapping_id: operation.mapping_id,
                    source_layer_id: operation.source_layer_id,
                    target: layer.target,
                    mask_sha256: operation.mask_sha256.clone(),
                    asset_id,
                    asset_sha256,
                    asset_media_type,
                    treatment_id: mapping.treatment_id,
                    image_production_mode: treatment.map(|treatment| treatment.production_mode),
                    algorithm_version: treatment
                        .map(|treatment| treatment.recipe.algorithm_version.clone()),
                    recipe_fingerprint: treatment.map(|treatment| treatment.recipe.fingerprint()),
                })
            })
        })
        .collect();
    ProductionTraceReport {
        format: PRODUCTION_TRACE_FORMAT.to_owned(),
        revision,
        coordinate_space: ProductionCoordinateSpace::BoardPhysicalUpright,
        manufacturer_profile: document.manufacturer_profile.clone(),
        manufacturer_profile_fingerprint: fingerprint(&document.manufacturer_profile),
        fabrication_input_sha256: resolved.build.input_sha256.clone(),
        fabrication_output_sha256: resolved.build.output_sha256.clone(),
        layers,
        operations,
    }
}

fn fingerprint(value: &impl Serialize) -> String {
    let canonical = serde_json::to_vec(value).expect("serializable production trace value");
    format!("{:x}", Sha256::digest(canonical))
}

fn active_physical_bounds(mask: &BitMask, grid: RasterGrid) -> Option<PhysicalBoundsUm> {
    let mut min_x = mask.width_px();
    let mut min_y = mask.height_px();
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..mask.height_px() {
        for x in 0..mask.width_px() {
            if mask.get(x, y).ok()? {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + 1);
                max_y = max_y.max(y + 1);
            }
        }
    }
    (min_x < max_x && min_y < max_y).then(|| PhysicalBoundsUm {
        min_x_um: grid.origin_x_um + i64::from(min_x.saturating_mul(grid.pixel_pitch_um)),
        min_y_um: grid.origin_y_um + i64::from(min_y.saturating_mul(grid.pixel_pitch_um)),
        max_x_um: grid.origin_x_um
            + i64::from(grid.width_um.min(max_x.saturating_mul(grid.pixel_pitch_um))),
        max_y_um: grid.origin_y_um
            + i64::from(
                grid.height_um
                    .min(max_y.saturating_mul(grid.pixel_pitch_um)),
            ),
    })
}

fn mask_topology(mask: &BitMask) -> MaskTopology {
    MaskTopology {
        island_count: component_count(mask, true, false),
        hole_count: component_count(mask, false, true),
    }
}

fn component_count(mask: &BitMask, target: bool, exclude_border: bool) -> u32 {
    let width = mask.width_px();
    let height = mask.height_px();
    let mut visited = vec![false; (u64::from(width) * u64::from(height)) as usize];
    let mut count = 0;
    for y in 0..height {
        for x in 0..width {
            let start = mask_index(width, x, y);
            if visited[start] || mask.get(x, y).ok() != Some(target) {
                continue;
            }
            let mut queue = VecDeque::from([(x, y)]);
            visited[start] = true;
            let mut touches_border = false;
            while let Some((current_x, current_y)) = queue.pop_front() {
                touches_border |= current_x == 0
                    || current_y == 0
                    || current_x + 1 == width
                    || current_y + 1 == height;
                for (next_x, next_y) in mask_neighbors(current_x, current_y, width, height) {
                    let next = mask_index(width, next_x, next_y);
                    if !visited[next] && mask.get(next_x, next_y).ok() == Some(target) {
                        visited[next] = true;
                        queue.push_back((next_x, next_y));
                    }
                }
            }
            if !exclude_border || !touches_border {
                count += 1;
            }
        }
    }
    count
}

fn mask_neighbors(x: u32, y: u32, width: u32, height: u32) -> impl Iterator<Item = (u32, u32)> {
    [
        x.checked_sub(1).map(|next| (next, y)),
        (x + 1 < width).then_some((x + 1, y)),
        y.checked_sub(1).map(|next| (x, next)),
        (y + 1 < height).then_some((x, y + 1)),
    ]
    .into_iter()
    .flatten()
}

fn mask_index(width: u32, x: u32, y: u32) -> usize {
    (u64::from(y) * u64::from(width) + u64::from(x)) as usize
}

#[cfg(test)]
mod tests {
    use crate::{
        AtelierDocument, ProjectBundle, ProjectBundleRasterizer, compile_fabrication_plan,
        resolve_fabrication_plan,
    };

    use super::build_production_trace;

    #[test]
    fn empty_board_trace_still_records_manufacturer_and_build_identity() {
        let bundle = ProjectBundle::new(AtelierDocument::new_card("trace", 1_000, 2_000));
        let plan = compile_fabrication_plan(&bundle.document).expect("plan");
        let mut rasterizer = ProjectBundleRasterizer::new(&bundle).expect("rasterizer");
        let resolved = resolve_fabrication_plan(&plan, 100, &mut rasterizer).expect("resolved");

        let trace = build_production_trace(9, &bundle.document, &resolved);

        assert_eq!(trace.revision, 9);
        assert_eq!(trace.fabrication_input_sha256, resolved.build.input_sha256);
        assert_eq!(
            trace.fabrication_output_sha256,
            resolved.build.output_sha256
        );
        assert_eq!(
            trace.manufacturer_profile,
            bundle.document.manufacturer_profile
        );
        assert!(trace.operations.is_empty());
    }
}
