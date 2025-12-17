/// Mock game entity
#[derive(Clone)]
pub struct Entity {
    pub id: u32,
    pub position: (f32, f32, f32),
    pub velocity: (f32, f32, f32),
    pub health: f32,
    pub active: bool,
}

impl Entity {
    pub fn new(id: u32) -> Self {
        Entity {
            id,
            position: (0.0, 0.0, 0.0),
            velocity: (1.0, 1.0, 1.0),
            health: 100.0,
            active: true,
        }
    }

    pub fn update(&mut self, dt: f32) {
        if self.active {
            self.position.0 += self.velocity.0 * dt;
            self.position.1 += self.velocity.1 * dt;
            self.position.2 += self.velocity.2 * dt;
        }
    }
}

// Benchmarks are conditionally compiled - only when built with cfg(test)
#[cfg(test)]
mod benchmarks {
    use super::*;
    use simplebench_macros::bench;

    // Simple benchmark (no setup) - entity creation IS what we're measuring
    #[bench]
    fn bench_entity_creation() {
        let mut entities = Vec::new();
        for i in 0..2000 {
            entities.push(Entity::new(i));
        }
    }

    // Setup helper: creates entities for update benchmarks
    fn create_entities(count: u32) -> Vec<Entity> {
        (0..count).map(Entity::new).collect()
    }

    // Setup pattern: create entities once, measure updates separately
    #[bench(setup = || create_entities(1000))]
    fn bench_entity_update_loop(entities: &Vec<Entity>) {
        // Clone to allow mutation without affecting setup data
        let mut entities = entities.clone();

        // Simulate 10 frame updates
        for _ in 0..10 {
            for entity in &mut entities {
                entity.update(0.016); // ~60 FPS
            }
        }
    }

    // Setup helper: creates entities with varied properties for filtering
    fn create_filterable_entities() -> Vec<Entity> {
        (0..3000u32)
            .map(|i| {
                let mut e = Entity::new(i);
                e.active = i % 3 != 0; // ~2/3 active
                e.health = (i % 100) as f32;
                e
            })
            .collect()
    }

    // Setup pattern: entity creation is expensive, filtering is what we measure
    #[bench(setup = create_filterable_entities)]
    fn bench_entity_filtering(entities: &Vec<Entity>) {
        // Filter active entities with health > 50
        let _active_healthy: Vec<_> = entities
            .iter()
            .filter(|e| e.active && e.health > 50.0)
            .collect();
    }
}
