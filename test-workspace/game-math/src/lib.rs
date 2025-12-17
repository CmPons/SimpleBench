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

// Test helpers using dev-dependencies
#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    /// Generate random test vectors - uses rand dev-dependency
    pub fn random_vectors(count: usize) -> Vec<Vec3> {
        let mut rng = rand::thread_rng();
        (0..count)
            .map(|_| Vec3::new(
                rng.gen_range(-100.0..100.0),
                rng.gen_range(-100.0..100.0),
                rng.gen_range(-100.0..100.0),
            ))
            .collect()
    }

    #[test]
    fn test_normalize() {
        let vectors = random_vectors(10);
        for v in vectors {
            let n = v.normalize();
            let len = n.length_squared().sqrt();
            assert!((len - 1.0).abs() < 0.001);
        }
    }
}

// Benchmarks using test helpers (which use dev-dependencies)
#[cfg(test)]
mod benchmarks {
    use super::*;
    use super::tests::random_vectors; // Uses rand transitively!
    use simplebench_macros::bench;

    // Setup pattern: random_vectors runs ONCE, not on every iteration
    // Before: random_vectors ran 1,000,000+ times (1000 samples × 1000 iterations)
    // After: random_vectors runs exactly once, then normalize is measured
    #[bench(setup = || random_vectors(1000))]
    fn bench_vec3_normalize(vectors: &Vec<Vec3>) {
        for v in vectors {
            let _normalized = v.normalize();
        }
    }

    #[bench(setup = || random_vectors(100))]
    fn bench_vec3_cross_product(vectors: &Vec<Vec3>) {
        for i in 0..vectors.len() - 1 {
            let _result = vectors[i].cross(&vectors[i + 1]);
        }
    }

    #[bench(setup = || random_vectors(500))]
    fn bench_matrix_transform_batch(vectors: &Vec<Vec3>) {
        // Simple rotation-like transformation
        for v in vectors {
            let _transformed = Vec3::new(v.x * 0.866 - v.y * 0.5, v.x * 0.5 + v.y * 0.866, v.z);
        }
    }
}
