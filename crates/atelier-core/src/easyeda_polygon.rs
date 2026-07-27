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
    /// True only when every bounded smoothing candidate was geometrically
    /// unsafe and this ring retained the exact raster boundary.
    pub used_exact_raster_fallback: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct VectorVertex {
    x: f64,
    y: f64,
}

impl From<GridVertex> for VectorVertex {
    fn from(point: GridVertex) -> Self {
        Self {
            x: f64::from(point.x),
            y: f64::from(point.y),
        }
    }
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
            let (vector_ring, used_exact_raster_fallback) = vectorize_raster_staircase_ring(ring);
            let mut points = vector_ring
                .iter()
                .map(|point| {
                    let mut x = edge_um_f64(
                        grid.origin_x_um,
                        grid.width_um,
                        grid.pixel_pitch_um,
                        point.x,
                    );
                    let mut y = edge_um_f64(
                        grid.origin_y_um,
                        grid.height_um,
                        grid.pixel_pitch_um,
                        point.y,
                    );
                    if transform.mirror_x {
                        x = grid.origin_x_um as f64 + f64::from(grid.width_um)
                            - (x - grid.origin_x_um as f64);
                    }
                    if transform.invert_y {
                        y = grid.origin_y_um as f64 + f64::from(grid.height_um)
                            - (y - grid.origin_y_um as f64);
                    }
                    (um_to_mil_f64(x), um_to_mil_f64(y))
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
            Ok(EasyedaFillPath {
                points_mil: points,
                used_exact_raster_fallback,
            })
        })
        .collect()
}

/// Converts a binary mask boundary into a subpixel vector contour.
///
/// Two bounded corner-cutting passes remove the visual ninety-degree cadence
/// created by nearest-neighbour source scaling. A conservative
/// Douglas-Peucker pass then removes redundant samples. The selected trim and
/// simplification tolerances keep the resulting contour within one formal
/// production pixel of the traced boundary while allowing coordinates between
/// raster-grid vertices, matching the geometry produced by EasyEDA's native
/// image importer.
///
/// Ring grouping happens before this function is called. Candidates are tried
/// from strongest to weakest so a locally unsafe simplification does not
/// immediately expose the exact raster staircase. The exact boundary remains
/// the final topology-preserving fallback.
fn vectorize_raster_staircase_ring(ring: &[GridVertex]) -> (Vec<VectorVertex>, bool) {
    const CORNER_TRIM_PX: f64 = 0.35;
    const SIMPLIFY_DEVIATION_PX: f64 = 0.5;

    if ring.len() <= 5 || ring.first() != ring.last() {
        return (
            ring.iter().copied().map(VectorVertex::from).collect(),
            false,
        );
    }
    let exact = ring
        .iter()
        .copied()
        .map(VectorVertex::from)
        .collect::<Vec<_>>();
    let rounded_once = cut_vector_corners(&exact, CORNER_TRIM_PX);
    let rounded_twice = cut_vector_corners(&rounded_once, CORNER_TRIM_PX);
    let candidates = [
        simplify_closed_vector_ring(&rounded_twice, SIMPLIFY_DEVIATION_PX.powi(2)),
        simplify_closed_vector_ring(&rounded_once, (SIMPLIFY_DEVIATION_PX / 2.0).powi(2)),
        rounded_once,
    ];
    for candidate in candidates {
        if vector_candidate_is_safe(&candidate, ring) {
            return (candidate, false);
        }
    }
    (exact, true)
}

fn vector_candidate_is_safe(candidate: &[VectorVertex], exact: &[GridVertex]) -> bool {
    candidate.len() >= 4
        && vector_ring_area(candidate).signum() == ring_area(exact).signum() as f64
        && !vector_ring_has_self_intersection(candidate)
}

fn cut_vector_corners(ring: &[VectorVertex], trim: f64) -> Vec<VectorVertex> {
    if ring.len() <= 4 || ring.first() != ring.last() {
        return ring.to_vec();
    }
    let unique = &ring[..ring.len() - 1];
    let mut rounded = Vec::with_capacity(unique.len() * 2 + 1);
    for index in 0..unique.len() {
        let previous = unique[(index + unique.len() - 1) % unique.len()];
        let current = unique[index];
        let next = unique[(index + 1) % unique.len()];
        let incoming = (current.x - previous.x, current.y - previous.y);
        let outgoing = (next.x - current.x, next.y - current.y);
        let incoming_length = incoming.0.hypot(incoming.1);
        let outgoing_length = outgoing.0.hypot(outgoing.1);
        let cross = incoming.0 * outgoing.1 - incoming.1 * outgoing.0;
        if incoming_length <= f64::EPSILON || outgoing_length <= f64::EPSILON || cross.abs() <= 1e-9
        {
            push_distinct(&mut rounded, current);
            continue;
        }
        let incoming_trim = trim.min(incoming_length / 2.0);
        let outgoing_trim = trim.min(outgoing_length / 2.0);
        push_distinct(
            &mut rounded,
            VectorVertex {
                x: current.x - incoming.0 / incoming_length * incoming_trim,
                y: current.y - incoming.1 / incoming_length * incoming_trim,
            },
        );
        push_distinct(
            &mut rounded,
            VectorVertex {
                x: current.x + outgoing.0 / outgoing_length * outgoing_trim,
                y: current.y + outgoing.1 / outgoing_length * outgoing_trim,
            },
        );
    }
    close_vector_ring(&mut rounded);
    rounded
}

fn simplify_closed_vector_ring(
    ring: &[VectorVertex],
    max_distance_squared: f64,
) -> Vec<VectorVertex> {
    if ring.len() <= 5 || ring.first() != ring.last() {
        return ring.to_vec();
    }
    let unique = &ring[..ring.len() - 1];
    let anchor = unique[0];
    let opposite_index = unique
        .iter()
        .enumerate()
        .max_by(|(_, first), (_, second)| {
            vector_squared_distance(anchor, **first)
                .total_cmp(&vector_squared_distance(anchor, **second))
        })
        .map(|(index, _)| index)
        .unwrap_or(0);
    if opposite_index == 0 {
        return ring.to_vec();
    }

    let first_half =
        simplify_open_vector_polyline(&unique[..=opposite_index], max_distance_squared);
    let mut wrapped_half = unique[opposite_index..].to_vec();
    wrapped_half.push(anchor);
    let second_half = simplify_open_vector_polyline(&wrapped_half, max_distance_squared);

    let mut candidate = first_half;
    candidate.extend(second_half.into_iter().skip(1));
    close_vector_ring(&mut candidate);
    candidate
}

fn simplify_open_vector_polyline(
    points: &[VectorVertex],
    max_distance_squared: f64,
) -> Vec<VectorVertex> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut pending = vec![(0usize, points.len() - 1)];

    while let Some((start, end)) = pending.pop() {
        if end <= start + 1 {
            continue;
        }
        let mut farthest = None;
        let mut farthest_distance = max_distance_squared;
        for index in start + 1..end {
            let distance =
                vector_point_segment_distance_squared(points[index], points[start], points[end]);
            if distance > farthest_distance {
                farthest = Some(index);
                farthest_distance = distance;
            }
        }
        if let Some(index) = farthest {
            keep[index] = true;
            pending.push((start, index));
            pending.push((index, end));
        }
    }

    points
        .iter()
        .zip(keep)
        .filter_map(|(point, keep)| keep.then_some(*point))
        .collect()
}

fn push_distinct(points: &mut Vec<VectorVertex>, point: VectorVertex) {
    if points.last().copied() != Some(point) {
        points.push(point);
    }
}

fn close_vector_ring(points: &mut Vec<VectorVertex>) {
    if points.first() != points.last()
        && let Some(first) = points.first().copied()
    {
        points.push(first);
    }
}

fn vector_squared_distance(a: VectorVertex, b: VectorVertex) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

fn vector_point_segment_distance_squared(
    point: VectorVertex,
    start: VectorVertex,
    end: VectorVertex,
) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0.0 {
        return vector_squared_distance(point, start);
    }
    let t =
        (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared).clamp(0.0, 1.0);
    let nearest_x = start.x + t * dx;
    let nearest_y = start.y + t * dy;
    (point.x - nearest_x).powi(2) + (point.y - nearest_y).powi(2)
}

fn vector_ring_has_self_intersection(ring: &[VectorVertex]) -> bool {
    let segment_count = ring.len().saturating_sub(1);
    for first in 0..segment_count {
        for second in first + 1..segment_count {
            if second == first + 1 || (first == 0 && second + 1 == segment_count) {
                continue;
            }
            if vector_segments_intersect(
                ring[first],
                ring[first + 1],
                ring[second],
                ring[second + 1],
            ) {
                return true;
            }
        }
    }
    false
}

fn vector_segments_intersect(
    a_start: VectorVertex,
    a_end: VectorVertex,
    b_start: VectorVertex,
    b_end: VectorVertex,
) -> bool {
    let o1 = vector_orientation(a_start, a_end, b_start);
    let o2 = vector_orientation(a_start, a_end, b_end);
    let o3 = vector_orientation(b_start, b_end, a_start);
    let o4 = vector_orientation(b_start, b_end, a_end);
    if o1.abs() <= 1e-9 && vector_point_on_segment(b_start, a_start, a_end)
        || o2.abs() <= 1e-9 && vector_point_on_segment(b_end, a_start, a_end)
        || o3.abs() <= 1e-9 && vector_point_on_segment(a_start, b_start, b_end)
        || o4.abs() <= 1e-9 && vector_point_on_segment(a_end, b_start, b_end)
    {
        return true;
    }
    (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0)
}

fn vector_orientation(a: VectorVertex, b: VectorVertex, c: VectorVertex) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn vector_point_on_segment(point: VectorVertex, start: VectorVertex, end: VectorVertex) -> bool {
    point.x >= start.x.min(end.x) - 1e-9
        && point.x <= start.x.max(end.x) + 1e-9
        && point.y >= start.y.min(end.y) - 1e-9
        && point.y <= start.y.max(end.y) + 1e-9
}

fn vector_ring_area(ring: &[VectorVertex]) -> f64 {
    ring.windows(2)
        .map(|edge| edge[0].x * edge[1].y - edge[1].x * edge[0].y)
        .sum::<f64>()
        / 2.0
}

fn edge_um_f64(origin_um: i64, extent_um: u32, pitch_um: u32, edge: f64) -> f64 {
    origin_um as f64 + (edge * f64::from(pitch_um)).clamp(0.0, f64::from(extent_um))
}

fn um_to_mil_f64(value_um: f64) -> f64 {
    value_um / 25.4
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
