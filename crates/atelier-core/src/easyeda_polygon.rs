//! Deterministic production-mask polygonization for static EasyEDA `FILL`s.
//!
//! This module consumes only resolved, bit-packed fabrication masks and their
//! physical grid. It deliberately has no dependency on documents, assets,
//! text, recipes, UI state, or the public archive writer. The boundary tracing
//! and outer-ring/hole grouping are derived from the MIT-licensed
//! PCB_lightgraph Neo `easyeda.rs` implementation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BitMask, FabricationResolveError, RasterGrid};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridVertex {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticFill {
    /// The first ring is the clockwise exterior; later rings are holes.
    pub rings: Vec<Vec<GridVertex>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardToEasyedaTransform {
    /// EasyEDA's PCB coordinate system is bottom-origin, unlike the raster grid.
    pub invert_y: bool,
    /// A format adapter may request X mirroring; callers must not derive this
    /// from the viewed card side.
    pub mirror_x: bool,
}

impl Default for BoardToEasyedaTransform {
    fn default() -> Self {
        Self {
            invert_y: true,
            mirror_x: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyedaFillPath {
    /// Closed path coordinates in EasyEDA's mil unit, ready for a static FILL.
    pub points_mil: Vec<(f64, f64)>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolygonizeError {
    #[error(transparent)]
    Mask(#[from] FabricationResolveError),
    #[error("mask dimensions do not match raster grid")]
    DimensionsMismatch,
    #[error("raster grid has invalid physical dimensions")]
    InvalidGrid,
}

pub fn polygonize_mask(
    mask: &BitMask,
    grid: &RasterGrid,
) -> Result<Vec<StaticFill>, PolygonizeError> {
    if mask.width_px() != grid.width_px || mask.height_px() != grid.height_px {
        return Err(PolygonizeError::DimensionsMismatch);
    }
    if grid.width_um == 0 || grid.height_um == 0 || grid.pixel_pitch_um == 0 {
        return Err(PolygonizeError::InvalidGrid);
    }
    let mut outgoing = BTreeMap::<GridVertex, Vec<GridVertex>>::new();
    for y in 0..mask.height_px() {
        for x in 0..mask.width_px() {
            if !mask.get(x, y)? {
                continue;
            }
            if y == 0 || !mask.get(x, y - 1)? {
                add_edge(
                    &mut outgoing,
                    GridVertex { x, y },
                    GridVertex { x: x + 1, y },
                );
            }
            if x + 1 == mask.width_px() || !mask.get(x + 1, y)? {
                add_edge(
                    &mut outgoing,
                    GridVertex { x: x + 1, y },
                    GridVertex { x: x + 1, y: y + 1 },
                );
            }
            if y + 1 == mask.height_px() || !mask.get(x, y + 1)? {
                add_edge(
                    &mut outgoing,
                    GridVertex { x: x + 1, y: y + 1 },
                    GridVertex { x, y: y + 1 },
                );
            }
            if x == 0 || !mask.get(x - 1, y)? {
                add_edge(
                    &mut outgoing,
                    GridVertex { x, y: y + 1 },
                    GridVertex { x, y },
                );
            }
        }
    }
    let rings = trace_rings(&outgoing)
        .into_iter()
        .map(simplify_collinear_ring)
        .collect::<Vec<_>>();
    let mut fills = rings
        .iter()
        .filter(|ring| ring_area(ring) > 0)
        .cloned()
        .map(|outer| StaticFill { rings: vec![outer] })
        .collect::<Vec<_>>();
    for hole in rings.into_iter().filter(|ring| ring_area(ring) < 0) {
        let Some(point) = hole.first().copied() else {
            continue;
        };
        if let Some(fill) = fills
            .iter_mut()
            .filter(|fill| contains(&fill.rings[0], point))
            .min_by_key(|fill| ring_area(&fill.rings[0]).unsigned_abs())
        {
            fill.rings.push(hole);
        }
    }
    fills.sort_by_key(|fill| fill.rings[0][0]);
    Ok(fills)
}

pub fn easyeda_paths(
    fill: &StaticFill,
    grid: &RasterGrid,
    transform: BoardToEasyedaTransform,
) -> Result<Vec<EasyedaFillPath>, PolygonizeError> {
    if grid.width_um == 0 || grid.height_um == 0 || grid.pixel_pitch_um == 0 {
        return Err(PolygonizeError::InvalidGrid);
    }
    fill.rings
        .iter()
        .map(|ring| {
            let mut points = ring
                .iter()
                .map(|point| {
                    let mut x = edge_um(
                        grid.origin_x_um,
                        grid.width_um,
                        grid.pixel_pitch_um,
                        point.x,
                    );
                    let mut y = edge_um(
                        grid.origin_y_um,
                        grid.height_um,
                        grid.pixel_pitch_um,
                        point.y,
                    );
                    if transform.mirror_x {
                        x = grid.origin_x_um + i64::from(grid.width_um) - (x - grid.origin_x_um);
                    }
                    if transform.invert_y {
                        y = grid.origin_y_um + i64::from(grid.height_um) - (y - grid.origin_y_um);
                    }
                    (um_to_mil(x), um_to_mil(y))
                })
                .collect::<Vec<_>>();
            if transform.mirror_x ^ transform.invert_y {
                points.reverse();
            }
            if points.first() != points.last() {
                if let Some(first) = points.first().copied() {
                    points.push(first);
                }
            }
            Ok(EasyedaFillPath { points_mil: points })
        })
        .collect()
}

fn edge_um(origin_um: i64, extent_um: u32, pitch_um: u32, edge: u32) -> i64 {
    origin_um + i64::from((u64::from(edge) * u64::from(pitch_um)).min(u64::from(extent_um)) as u32)
}

fn um_to_mil(value_um: i64) -> f64 {
    value_um as f64 / 25.4
}

fn add_edge(
    outgoing: &mut BTreeMap<GridVertex, Vec<GridVertex>>,
    start: GridVertex,
    end: GridVertex,
) {
    outgoing.entry(start).or_default().push(end);
}

fn trace_rings(outgoing: &BTreeMap<GridVertex, Vec<GridVertex>>) -> Vec<Vec<GridVertex>> {
    let mut remaining = outgoing
        .iter()
        .flat_map(|(start, ends)| ends.iter().map(|end| (*start, *end)))
        .collect::<BTreeSet<_>>();
    let mut rings = Vec::new();
    while let Some(&(start, first_end)) = remaining.first() {
        remaining.remove(&(start, first_end));
        let mut ring = vec![start, first_end];
        let mut previous = start;
        let mut current = first_end;
        while current != start {
            let incoming = direction(previous, current);
            let Some(next) = outgoing
                .get(&current)
                .into_iter()
                .flatten()
                .filter(|candidate| remaining.contains(&(current, **candidate)))
                .min_by_key(|candidate| turn_priority(incoming, direction(current, **candidate)))
                .copied()
            else {
                break;
            };
            remaining.remove(&(current, next));
            previous = current;
            current = next;
            ring.push(current);
        }
        if ring.last() == Some(&start) {
            rings.push(ring);
        }
    }
    rings
}

fn simplify_collinear_ring(ring: Vec<GridVertex>) -> Vec<GridVertex> {
    if ring.len() <= 5 || ring.first() != ring.last() {
        return ring;
    }
    let unique = &ring[..ring.len() - 1];
    let mut simplified = Vec::with_capacity(unique.len());
    for index in 0..unique.len() {
        let previous = unique[(index + unique.len() - 1) % unique.len()];
        let current = unique[index];
        let next = unique[(index + 1) % unique.len()];
        let incoming_horizontal = previous.y == current.y;
        let outgoing_horizontal = current.y == next.y;
        if incoming_horizontal != outgoing_horizontal {
            simplified.push(current);
        }
    }
    if let Some(first) = simplified.first().copied() {
        simplified.push(first);
    }
    simplified
}

fn direction(start: GridVertex, end: GridVertex) -> u8 {
    match (
        i64::from(end.x) - i64::from(start.x),
        i64::from(end.y) - i64::from(start.y),
    ) {
        (1, 0) => 0,
        (0, 1) => 1,
        (-1, 0) => 2,
        (0, -1) => 3,
        _ => unreachable!("boundary edges are unit axis-aligned segments"),
    }
}

fn turn_priority(incoming: u8, outgoing: u8) -> u8 {
    match (outgoing + 4 - incoming) % 4 {
        1 => 0,
        0 => 1,
        3 => 2,
        2 => 3,
        _ => unreachable!(),
    }
}

fn ring_area(ring: &[GridVertex]) -> i64 {
    ring.windows(2)
        .map(|edge| {
            i64::from(edge[0].x) * i64::from(edge[1].y)
                - i64::from(edge[1].x) * i64::from(edge[0].y)
        })
        .sum::<i64>()
        / 2
}

fn contains(ring: &[GridVertex], point: GridVertex) -> bool {
    let (x, y) = (f64::from(point.x), f64::from(point.y));
    let mut inside = false;
    for edge in ring.windows(2) {
        let (x1, y1) = (f64::from(edge[0].x), f64::from(edge[0].y));
        let (x2, y2) = (f64::from(edge[1].x), f64::from(edge[1].y));
        if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
            inside = !inside;
        }
    }
    inside
}
