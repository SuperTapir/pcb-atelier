use atelier_core::{
    BitMask, BoardOutline, BoardToEasyedaTransform, RasterGrid, easyeda_paths, polygonize_mask,
};

fn grid(width_um: u32, height_um: u32, pitch_um: u32) -> RasterGrid {
    RasterGrid::for_board(
        &BoardOutline::Rectangle {
            width_um,
            height_um,
        },
        pitch_um,
    )
    .expect("grid")
}

fn mask(width: u32, height: u32, active: &[(u32, u32)]) -> BitMask {
    let mut mask = BitMask::new(width, height).expect("mask");
    for &(x, y) in active {
        mask.set(x, y, true).expect("pixel");
    }
    mask
}

fn signed_area(points: &[(f64, f64)]) -> f64 {
    points
        .windows(2)
        .map(|edge| edge[0].0 * edge[1].1 - edge[1].0 * edge[0].1)
        .sum::<f64>()
        / 2.0
}

fn point_segment_distance(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> f64 {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0.0 {
        return ((point.0 - start.0).powi(2) + (point.1 - start.1).powi(2)).sqrt();
    }
    let t =
        (((point.0 - start.0) * dx + (point.1 - start.1) * dy) / length_squared).clamp(0.0, 1.0);
    ((point.0 - (start.0 + t * dx)).powi(2) + (point.1 - (start.1 + t * dy)).powi(2)).sqrt()
}

fn orientation(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn path_has_self_intersection(points: &[(f64, f64)]) -> bool {
    let segment_count = points.len() - 1;
    for first in 0..segment_count {
        for second in first + 1..segment_count {
            if second == first + 1 || (first == 0 && second + 1 == segment_count) {
                continue;
            }
            let a = (points[first], points[first + 1]);
            let b = (points[second], points[second + 1]);
            let o1 = orientation(a.0, a.1, b.0);
            let o2 = orientation(a.0, a.1, b.1);
            let o3 = orientation(b.0, b.1, a.0);
            let o4 = orientation(b.0, b.1, a.1);
            if (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0) {
                return true;
            }
        }
    }
    false
}

#[test]
fn polygonize_keeps_disconnected_islands_separate() {
    let fills = polygonize_mask(&mask(5, 3, &[(0, 0), (4, 2)]), &grid(50, 30, 10)).expect("fills");
    assert_eq!(fills.len(), 2);
    assert!(fills.iter().all(|fill| fill.rings.len() == 1));
}

#[test]
fn polygonize_assigns_an_inner_empty_region_to_its_outer_fill() {
    let mut pixels = Vec::new();
    for y in 0..3 {
        for x in 0..3 {
            if (x, y) != (1, 1) {
                pixels.push((x, y));
            }
        }
    }
    let fills = polygonize_mask(&mask(3, 3, &pixels), &grid(30, 30, 10)).expect("fills");
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].rings.len(), 2);
}

#[test]
fn polygonize_preserves_board_edges_and_partial_final_pixels_in_physical_coordinates() {
    let grid = grid(25, 15, 10);
    let fills = polygonize_mask(&mask(3, 2, &[(2, 1)]), &grid).expect("fills");
    let paths = easyeda_paths(&fills[0], &grid, BoardToEasyedaTransform::default()).expect("paths");
    let points = &paths[0].points_mil;
    assert!(
        points
            .iter()
            .any(|&(x, _)| (x - 25.0 / 25.4).abs() < 0.000_001)
    );
    assert!(points.iter().any(|&(_, y)| (y - 0.0).abs() < 0.000_001));
    assert_eq!(points.first(), points.last());
}

#[test]
fn back_side_geometry_is_not_pre_mirrored() {
    let grid = grid(40, 20, 10);
    let fills = polygonize_mask(&mask(4, 2, &[(0, 0)]), &grid).expect("fills");
    let paths = easyeda_paths(&fills[0], &grid, BoardToEasyedaTransform::default()).expect("paths");
    let max_x = paths[0]
        .points_mil
        .iter()
        .map(|(x, _)| *x)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (max_x - 10.0 / 25.4).abs() < 0.000_001,
        "board-coordinate left feature must stay left"
    );
}

#[test]
fn polygonize_removes_collinear_pixel_edges_from_rectangles() {
    let active = (0..3)
        .flat_map(|y| (0..4).map(move |x| (x, y)))
        .collect::<Vec<_>>();
    let fills = polygonize_mask(&mask(4, 3, &active), &grid(40, 30, 10)).expect("fills");

    assert_eq!(fills.len(), 1);
    assert_eq!(
        fills[0].rings[0].len(),
        5,
        "rectangle should contain four corners and the repeated closing point"
    );
}

#[test]
fn easyeda_paths_replace_raster_stair_steps_with_bounded_vector_segments() {
    // A 45-degree half-plane has a one-pixel staircase when its mask boundary
    // is traced literally. The EDA handoff should express that sampled edge as
    // a continuous diagonal vector segment.
    let active = (0..16)
        .flat_map(|y| (0..16).filter(move |x| *x <= y).map(move |x| (x, y)))
        .collect::<Vec<_>>();
    let raster_grid = grid(400, 400, 25);
    let fills = polygonize_mask(&mask(16, 16, &active), &raster_grid).expect("fills");
    let paths =
        easyeda_paths(&fills[0], &raster_grid, BoardToEasyedaTransform::default()).expect("paths");
    let points = &paths[0].points_mil;

    assert!(
        points.windows(2).any(|edge| {
            let dx = (edge[1].0 - edge[0].0).abs();
            let dy = (edge[1].1 - edge[0].1).abs();
            dx > 0.000_001 && dy > 0.000_001
        }),
        "the sampled diagonal must not remain a horizontal/vertical staircase"
    );
    assert!(
        points.len() <= 8,
        "a straight sampled diagonal should simplify to a small vector polygon, got {} points",
        points.len()
    );
}

#[test]
fn vector_simplification_preserves_donut_topology_winding_and_one_pixel_error_bound() {
    let active = (0..32)
        .flat_map(|y| {
            (0..32).filter_map(move |x| {
                let dx = i32::try_from(x).unwrap() - 16;
                let dy = i32::try_from(y).unwrap() - 16;
                let radius_squared = dx * dx + dy * dy;
                (25..=169).contains(&radius_squared).then_some((x, y))
            })
        })
        .collect::<Vec<_>>();
    let raster_grid = grid(800, 800, 25);
    let fills = polygonize_mask(&mask(32, 32, &active), &raster_grid).expect("fills");
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].rings.len(), 2);

    let paths =
        easyeda_paths(&fills[0], &raster_grid, BoardToEasyedaTransform::default()).expect("paths");
    assert_eq!(paths.len(), 2);
    assert!(signed_area(&paths[0].points_mil) * signed_area(&paths[1].points_mil) < 0.0);
    assert!(
        paths
            .iter()
            .all(|path| !path_has_self_intersection(&path.points_mil))
    );
    let one_pixel_mil = 25.0 / 25.4;
    for path in &paths {
        let residual_pixel_steps = path
            .points_mil
            .windows(2)
            .filter(|edge| {
                let dx = (edge[1].0 - edge[0].0).abs();
                let dy = (edge[1].1 - edge[0].1).abs();
                (dx < 0.000_001 || dy < 0.000_001) && dx.max(dy) <= one_pixel_mil + 0.000_001
            })
            .count();
        assert!(
            residual_pixel_steps <= 8,
            "smooth circular image contours must not retain {residual_pixel_steps} one-pixel staircase edges"
        );
    }

    for (ring, path) in fills[0].rings.iter().zip(&paths) {
        let simplified_grid_points = path
            .points_mil
            .iter()
            .map(|&(x_mil, y_mil)| (x_mil * 25.4 / 25.0, (800.0 - y_mil * 25.4) / 25.0))
            .collect::<Vec<_>>();
        let max_error = ring
            .iter()
            .map(|point| {
                simplified_grid_points
                    .windows(2)
                    .map(|edge| {
                        point_segment_distance(
                            (f64::from(point.x), f64::from(point.y)),
                            edge[0],
                            edge[1],
                        )
                    })
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max);
        assert!(
            max_error <= 1.0 + 0.000_001,
            "vectorized ring drifted by {max_error} pixels"
        );

        let exact_grid_points = ring
            .iter()
            .map(|point| (f64::from(point.x), f64::from(point.y)))
            .collect::<Vec<_>>();
        let reverse_max_error = simplified_grid_points
            .windows(2)
            .flat_map(|edge| {
                [
                    edge[0],
                    ((edge[0].0 + edge[1].0) / 2.0, (edge[0].1 + edge[1].1) / 2.0),
                ]
            })
            .map(|point| {
                exact_grid_points
                    .windows(2)
                    .map(|edge| point_segment_distance(point, edge[0], edge[1]))
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max);
        assert!(
            reverse_max_error <= 1.0 + 0.000_001,
            "vectorized ring invented geometry {reverse_max_error} pixels away from the formal boundary"
        );
    }
}

#[test]
fn source_scaled_curves_export_as_subpixel_non_axis_aligned_contours() {
    // Real imported images commonly contribute one source pixel to several
    // formal-production pixels. A nearest-neighbour threshold therefore
    // produces two-to-three-pixel plateaus even though the source contour is
    // visually curved. Keeping only raster-grid vertices leaves those
    // plateaus visible when EasyEDA zooms the static vector artwork.
    let active = (0..96)
        .flat_map(|y| {
            (0..96).filter_map(move |x| {
                let source_x = i32::try_from(x / 3).unwrap() - 16;
                let source_y = i32::try_from(y / 3).unwrap() - 16;
                (source_x * source_x + source_y * source_y <= 12 * 12).then_some((x, y))
            })
        })
        .collect::<Vec<_>>();
    let raster_grid = grid(2_400, 2_400, 25);
    let fills = polygonize_mask(&mask(96, 96, &active), &raster_grid).expect("fills");
    assert_eq!(fills.len(), 1);

    let paths =
        easyeda_paths(&fills[0], &raster_grid, BoardToEasyedaTransform::default()).expect("paths");
    let points = &paths[0].points_mil;
    let pitch_mil = 25.0 / 25.4;
    assert!(
        points.iter().any(|&(x, y)| {
            let grid_x = x / pitch_mil;
            let grid_y = (2_400.0 / 25.4 - y) / pitch_mil;
            (grid_x - grid_x.round()).abs() > 0.000_001
                || (grid_y - grid_y.round()).abs() > 0.000_001
        }),
        "image contours must contain subpixel vertices rather than remain locked to the 25 um grid"
    );

    let (axis_aligned, total) = points.windows(2).fold((0usize, 0usize), |counts, edge| {
        let dx = (edge[1].0 - edge[0].0).abs();
        let dy = (edge[1].1 - edge[0].1).abs();
        (
            counts.0 + usize::from(dx < 0.000_001 || dy < 0.000_001),
            counts.1 + 1,
        )
    });
    assert!(
        axis_aligned * 100 <= total * 30,
        "a source-scaled circular contour retained {axis_aligned}/{total} axis-aligned staircase edges"
    );
}
