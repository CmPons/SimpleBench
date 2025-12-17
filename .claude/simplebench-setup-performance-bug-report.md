# Bug Report: SimpleBench Measures Setup Code in Benchmark Loop

## Issue Summary
SimpleBench executes the entire benchmark function (including expensive setup code) for every iteration, rather than separating setup from the measured operation. This leads to misleading performance measurements and extremely long benchmark runs.

## Environment
- SimpleBench version: 1.0.6
- Rust version: (latest stable)
- OS: Linux

## Problem Description

SimpleBench measures the entire benchmark function body on every iteration, including expensive setup operations that should only run once. This causes:

1. **Misleading measurements**: Setup time is included in performance metrics
2. **Extremely long benchmark runs**: Setup code runs 1,000,000+ times
3. **Timeouts**: Benchmarks that should take seconds take hours

### Expected Behavior
Benchmark frameworks should separate setup from measurement:
```rust
// Setup (run once or minimally)
let test_data = create_expensive_test_data();

// Measured operation (run many times)
for _ in iterations {
    let result = operation_under_test(&test_data);
    black_box(result);
}
```

### Actual Behavior
SimpleBench measures the entire function body on every iteration:
```rust
#[bench]
fn my_benchmark() {
    // This runs 1,000,000+ times!
    let test_data = create_expensive_test_data(); // ← Problem!
    
    // This also runs 1,000,000+ times (correct)
    let result = operation_under_test(&test_data);
    black_box(result);
}
```

## Reproduction

### Test Case
```rust
use im::OrdMap;
use simplebench_macros::bench;
use std::hint::black_box;

#[bench]
fn im_ordmap_iterate_large() {
    // Expensive setup - creates 1000-entry map
    let data = (0..1000).map(|i| (format!("key_{}", i), i)).collect::<Vec<_>>();
    let mut map = OrdMap::new();
    for (key, value) in &data {
        map = map.update(key.clone(), *value);
    }
    
    // Fast operation being measured
    let mut sum = 0;
    for (_, v) in &map {
        sum += v;
    }
    black_box(sum);
}
```

### Measured Performance
- **Setup time**: 4.95ms (OrdMap creation)
- **Operation time**: 104μs (iteration)
- **Ratio**: Setup is 47x slower than operation!

### SimpleBench Execution
- **Warmup**: ~5 seconds = ~50,000 iterations
- **Measurement**: 1000 samples × 1000 iterations = 1,000,000 iterations
- **Total setup executions**: ~1,050,000
- **Total setup time**: 4.95ms × 1,050,000 = **5,202 seconds (86.7 minutes)**

### Result
Benchmark times out or reports misleading performance that includes setup overhead.

## Impact

### Performance Measurement Issues
- Benchmarks report "iteration time" that's actually "setup + iteration time"
- Impossible to isolate actual operation performance
- Setup-heavy benchmarks show artificially poor performance

### Usability Issues  
- Benchmarks with any non-trivial setup become unusable
- Users must structure code unnaturally to avoid setup
- Long benchmark runs that should take seconds take hours

### Common Affected Patterns
```rust
// All of these patterns are problematic:

#[bench] fn test_large_data() {
    let data = create_large_dataset(); // ← Expensive setup
    process_data(&data); // ← Fast operation
}

#[bench] fn test_complex_structure() {
    let structure = build_complex_structure(); // ← Expensive setup  
    query_structure(&structure); // ← Fast operation
}

#[bench] fn test_initialized_system() {
    let system = System::new_with_config(); // ← Expensive setup
    system.perform_operation(); // ← Fast operation
}
```

## Comparison with Other Frameworks

### Criterion.rs (Standard)
```rust
fn benchmark(c: &mut Criterion) {
    let test_data = create_expensive_data(); // Setup once
    
    c.bench_function("operation", |b| {
        b.iter(|| {
            operation(&test_data) // Only this is measured
        });
    });
}
```

### std::test::Bencher (Legacy)
```rust
#[bench]
fn bench_operation(b: &mut Bencher) {
    let test_data = create_expensive_data(); // Setup once
    
    b.iter(|| {
        operation(&test_data) // Only this is measured  
    });
}
```

## Suggested Solutions

### Option 1: Setup/Teardown Support
```rust
#[bench]
fn benchmark_with_setup() {
    simplebench::with_setup(
        || create_expensive_data(), // Setup
        |data| operation(data),     // Measured
        || cleanup()                // Teardown (optional)
    );
}
```

### Option 2: Closure-Based API
```rust
#[bench] 
fn benchmark_closure() {
    let test_data = create_expensive_data(); // Run once
    
    simplebench::bench(|| {
        operation(&test_data) // Only this measured
    });
}
```

### Option 3: Explicit Setup Annotation
```rust
#[bench]
fn benchmark_annotated() {
    #[setup] let test_data = create_expensive_data();
    
    // Only code after #[setup] is measured
    let result = operation(&test_data);
    black_box(result);
}
```

## Workaround

Currently, users must structure benchmarks unnaturally:

```rust
// Global setup (not ideal for isolated benchmarks)
static EXPENSIVE_DATA: Lazy<TestData> = Lazy::new(|| create_expensive_data());

#[bench]
fn benchmark_workaround() {
    let result = operation(&*EXPENSIVE_DATA);
    black_box(result);
}
```

## Related Issues

This fundamental design issue affects:
- All benchmarks with non-trivial setup
- Performance comparisons between different approaches
- Adoption of SimpleBench for real-world use cases

The setup overhead measurement makes SimpleBench unsuitable for many common benchmarking scenarios where setup and operation need to be separated.

## Priority

This is a **high priority** issue because:
1. It affects measurement accuracy (core functionality)
2. It makes many common benchmark patterns unusable
3. It causes user confusion about performance results
4. It blocks adoption for non-trivial use cases

## Test Verification

The issue can be verified by:
1. Running the reproduction case above
2. Measuring setup vs operation time separately
3. Observing the extreme benchmark runtime
4. Comparing results with other benchmark frameworks