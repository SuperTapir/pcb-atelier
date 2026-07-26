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
