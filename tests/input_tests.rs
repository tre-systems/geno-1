use app_web::input::{nearest_index_by_uvx, ray_sphere};

#[test]
fn ray_sphere_intersection_basic() {
    // Ray from origin pointing in +Z direction
    let ray_origin = glam::Vec3::ZERO;
    let ray_dir = glam::Vec3::new(0.0, 0.0, 1.0);

    // Sphere at (0, 0, 5) with radius 2
    let center = glam::Vec3::new(0.0, 0.0, 5.0);
    let radius = 2.0;

    let result = ray_sphere(ray_origin, ray_dir, center, radius);
    assert!(result.is_some());

    let t = result.unwrap();
    assert!(t > 0.0);
    assert!(t < 10.0); // Should hit before z=10
}

#[test]
fn ray_sphere_intersection_miss() {
    // Ray from origin pointing in +X direction
    let ray_origin = glam::Vec3::ZERO;
    let ray_dir = glam::Vec3::new(1.0, 0.0, 0.0);

    // Sphere at (0, 0, 5) with radius 2 (ray goes in X, sphere is in Z)
    let center = glam::Vec3::new(0.0, 0.0, 5.0);
    let radius = 2.0;

    let result = ray_sphere(ray_origin, ray_dir, center, radius);
    assert!(result.is_none());
}

#[test]
fn ray_sphere_intersection_tangent() {
    // Ray from origin pointing in +Z direction
    let ray_origin = glam::Vec3::ZERO;
    let ray_dir = glam::Vec3::new(0.0, 0.0, 1.0);

    // Sphere at (2, 0, 5) with radius 2 (ray goes through edge)
    let center = glam::Vec3::new(2.0, 0.0, 5.0);
    let radius = 2.0;

    let result = ray_sphere(ray_origin, ray_dir, center, radius);
    assert!(result.is_some());

    let t = result.unwrap();
    assert!(t > 0.0);
}

#[test]
fn ray_sphere_intersection_inside() {
    // Ray from inside sphere pointing outward
    let ray_origin = glam::Vec3::new(0.0, 0.0, 5.0);
    let ray_dir = glam::Vec3::new(1.0, 0.0, 0.0);

    // Sphere at (0, 0, 5) with radius 3
    let center = glam::Vec3::new(0.0, 0.0, 5.0);
    let radius = 3.0;

    let result = ray_sphere(ray_origin, ray_dir, center, radius);
    assert!(
        result.is_some(),
        "ray from inside sphere should still intersect"
    );
    let t = result.unwrap();
    assert!(t > 0.0);
    // Should hit at radius distance from center
    assert!((t - 3.0).abs() < 0.1);
}

#[test]
fn nearest_index_by_uvx_basic() {
    let voice_xs = vec![0.1, 0.3, 0.5, 0.7, 0.9];

    // Test exact matches
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.1), 0);
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.3), 1);
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.5), 2);
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.7), 3);
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.9), 4);
}

#[test]
fn nearest_index_by_uvx_interpolation() {
    let voice_xs = vec![0.1, 0.3, 0.5, 0.7, 0.9];

    // Test interpolation cases
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.2), 0); // Closer to 0.1
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.4), 1); // Closer to 0.3
                                                         // 0.6 is equidistant between 0.5 and 0.7, so result depends on iteration order
    let result_06 = nearest_index_by_uvx(&voice_xs, 0.6);
    // Should be either 0.5 or 0.7, not 0.9
    assert!(
        result_06 == 2 || result_06 == 3,
        "Expected 2 or 3, got {} (value: {})",
        result_06,
        voice_xs[result_06]
    );
    // 0.8 is equidistant between 0.7 and 0.9, so result depends on iteration order
    let result_08 = nearest_index_by_uvx(&voice_xs, 0.8);
    assert!(
        result_08 == 3 || result_08 == 4,
        "Expected 3 or 4, got {} (value: {})",
        result_08,
        voice_xs[result_08]
    );
}

#[test]
fn nearest_index_by_uvx_edge_cases() {
    let voice_xs = vec![0.1, 0.3, 0.5, 0.7, 0.9];

    // Test edge cases
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.0), 0); // Below range
    assert_eq!(nearest_index_by_uvx(&voice_xs, 1.0), 4); // Above range
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.05), 0); // Very close to 0.1
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.95), 4); // Very close to 0.9
}

#[test]
fn nearest_index_by_uvx_single_element() {
    let voice_xs = vec![0.5];

    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.0), 0);
    assert_eq!(nearest_index_by_uvx(&voice_xs, 0.5), 0);
    assert_eq!(nearest_index_by_uvx(&voice_xs, 1.0), 0);
}
