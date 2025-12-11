use simplebench_macros::mbench;

/// Simple 3D vector for testing
#[derive(Clone, Copy, Debug)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }

    pub fn dot(&self, other: &Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(&self, other: &Vec3) -> Vec3 {
        Vec3 {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    pub fn length_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn normalize(&self) -> Vec3 {
        let len = self.length_squared().sqrt();
        Vec3 {
            x: self.x / len,
            y: self.y / len,
            z: self.z / len,
        }
    }
}

#[mbench]
fn bench_vec3_normalize() {
    let mut vectors = Vec::new();
    for i in 0..1000 {
        vectors.push(Vec3::new(i as f32, i as f32 * 2.0, i as f32 * 3.0));
    }

    for v in &vectors {
        let _normalized = v.normalize();
    }
}

#[mbench]
fn bench_vec3_cross_product() {
    let v1 = Vec3::new(1.0, 2.0, 3.0);
    let v2 = Vec3::new(4.0, 5.0, 6.0);

    for _ in 0..5000 {
        let _result = v1.cross(&v2);
    }
}

#[mbench]
fn bench_matrix_transform_batch() {
    // Simulate transforming 500 vectors by a simple rotation
    let mut vectors = Vec::new();
    for i in 0..500 {
        vectors.push(Vec3::new(i as f32, i as f32 * 0.5, i as f32 * 0.25));
    }

    // Simple rotation-like transformation
    for v in &vectors {
        let _transformed = Vec3::new(v.x * 0.866 - v.y * 0.5, v.x * 0.5 + v.y * 0.866, v.z);
    }
}
