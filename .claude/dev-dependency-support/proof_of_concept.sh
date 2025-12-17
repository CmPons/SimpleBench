#!/bin/bash
# Proof of Concept: Dev-dependency support for SimpleBench
# Run from test-workspace directory

set -e

echo "=== Step 1: Build dev-deps via cargo test ==="
cargo clean --release
cargo test -p game-math --release --no-run --message-format=json > /tmp/cargo_output.json
echo "Captured $(wc -l < /tmp/cargo_output.json) lines of JSON"

echo -e "\n=== Step 2: Extract rlib paths ==="
# Get opt_level=3 rlibs and proc-macros (opt_level=0)
cat /tmp/cargo_output.json | jq -r '
  select(.reason == "compiler-artifact") |
  select(
    (.profile.opt_level == "3" or .profile.opt_level == 3) or
    (.target.kind[0] == "proc-macro")
  ) |
  select(.target.kind[0] != "custom-build") |
  select(.target.name != "game_math") |
  "\(.target.name)=\(.filenames[0])"
' | grep -E '\.(rlib|so)$' | sort -u > /tmp/externs.txt

echo "Found $(wc -l < /tmp/externs.txt) extern crates"

echo -e "\n=== Step 3: Manual rustc with all externs ==="
mkdir -p /tmp/test_rlib
EXTERNS=$(cat /tmp/externs.txt | sed 's/^/--extern /' | tr '\n' ' ')

rustc --edition=2021 \
  game-math/src/lib.rs \
  --crate-name game_math \
  --crate-type rlib \
  -C opt-level=3 \
  --cfg test \
  -L dependency=target/release/deps \
  $EXTERNS \
  --out-dir /tmp/test_rlib

echo "Created: $(ls -lh /tmp/test_rlib/libgame_math.rlib)"

echo -e "\n=== Step 4: Verify symbols ==="
echo "Benchmark symbols:"
nm -C /tmp/test_rlib/libgame_math.rlib 2>/dev/null | grep "__simplebench_wrapper" || echo "  (none)"

echo -e "\nRand usage:"
nm -C /tmp/test_rlib/libgame_math.rlib 2>/dev/null | grep "rand::" | head -3 || echo "  (none)"

echo -e "\n=== Step 5: Build and run test runner ==="
cat > /tmp/test_runner.rs << 'EOF'
extern crate game_math;
extern crate simplebench_runtime;

fn main() {
    println!("Collecting benchmarks...");
    let benchmarks: Vec<_> = inventory::iter::<simplebench_runtime::SimpleBench>().collect();
    println!("Found {} benchmarks:", benchmarks.len());
    for b in &benchmarks {
        println!("  - {}::{}", b.module, b.name);
    }
}
EOF

rustc --edition=2021 \
  /tmp/test_runner.rs \
  -C opt-level=0 \
  -L dependency=target/release/deps \
  -L /tmp/test_rlib \
  --extern game_math=/tmp/test_rlib/libgame_math.rlib \
  $EXTERNS \
  -o /tmp/test_runner

/tmp/test_runner

echo -e "\n=== SUCCESS ==="
