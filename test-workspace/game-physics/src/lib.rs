/// Simple AABB (Axis-Aligned Bounding Box)
#[derive(Clone, Copy)]
pub struct AABB {
    pub min_x: f32,
    pub min_y: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub max_z: f32,
}

impl AABB {
    pub fn new(min_x: f32, min_y: f32, min_z: f32, max_x: f32, max_y: f32, max_z: f32) -> Self {
        AABB {
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
        }
    }

    pub fn intersects(&self, other: &AABB) -> bool {
        self.max_x >= other.min_x
            && self.min_x <= other.max_x
            && self.max_y >= other.min_y
            && self.min_y <= other.max_y
            && self.max_z >= other.min_z
            && self.min_z <= other.max_z
    }

    pub fn contains_point(&self, x: f32, y: f32, z: f32) -> bool {
        x >= self.min_x
            && x <= self.max_x
            && y >= self.min_y
            && y <= self.max_y
            && z >= self.min_z
            && z <= self.max_z
    }
}

// Benchmarks are conditionally compiled - only when built with cfg(test)
#[cfg(test)]
mod benchmarks {
    use super::*;
    use simplebench_macros::bench;

    #[bench]
    fn bench_aabb_intersection_checks() {
        let mut boxes = Vec::new();
        for i in 0..200 {
            let offset = i as f32 * 0.5;
            boxes.push(AABB::new(offset, offset, offset, offset + 2.0, offset + 2.0, offset + 2.0));
        }

        let mut collision_count = 0;
        for i in 0..boxes.len() {
            for j in (i + 1)..boxes.len() {
                if boxes[i].intersects(&boxes[j]) {
                    collision_count += 1;
                }
            }
        }

        // Prevent optimization
        std::hint::black_box(collision_count);
    }

    #[bench]
    fn bench_point_containment_tests() {
        let aabb = AABB::new(-10.0, -10.0, -10.0, 10.0, 10.0, 10.0);

        let mut inside_count = 0;
        for x in -15..15 {
            for y in -15..15 {
                for z in -15..15 {
                    if aabb.contains_point(x as f32, y as f32, z as f32) {
                        inside_count += 1;
                    }
                }
            }
        }

        std::hint::black_box(inside_count);
    }
}
