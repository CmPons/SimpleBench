# Implementation Notes & Critical Reminders

**Last Updated**: 2025-12-10

---

## Critical Points for Phase 1 Implementation

### 🚨 MUST DELETE BASELINES FOR VARIANCE TESTING

**Before validating variance improvements, ALWAYS delete old baselines:**

```bash
cd test-workspace
rm -rf .benches/
```

**Why this is critical:**
- Old baselines use p90 format
- New implementation uses mean format
- Mixed formats will produce invalid variance measurements
- Phase 1 validation requires clean baselines to verify 0-3% variance target

**Where to delete:**
- `test-workspace/.benches/` - For integration testing
- Any other workspace `.benches/` directories being tested

---

## Breaking Changes Are Acceptable

**User is the sole developer** of this project, so:
- ✅ Breaking changes are expected and acceptable
- ✅ No need for backwards compatibility layers
- ✅ Can make aggressive improvements without migration paths
- ✅ Focus on correctness over compatibility

This allows us to:
1. Remove auto-scaling entirely (not just deprecate)
2. Change comparison metric from p90 → mean immediately
3. Change default configuration drastically (200 → 100,000 samples)
4. Require iterations field (remove Option wrapper)

---

## Variance Testing Procedure

**Correct procedure for verifying <3% variance:**

1. **Delete baselines:**
   ```bash
   rm -rf test-workspace/.benches/
   ```

2. **Run first benchmark (establishes baseline):**
   ```bash
   cd test-workspace
   cargo simplebench
   ```

3. **Run 5 more times and check variance:**
   ```bash
   for i in {2..6}; do
       echo "=== Run $i ==="
       cargo simplebench | tee run$i.txt
   done
   ```

4. **Verify:**
   - All benchmarks show <3% variance
   - No "NEW" baselines after first run
   - Consistent mean values across runs

**Common mistakes:**
- ❌ Not deleting old baselines first
- ❌ Running on system under load
- ❌ Comparing across different baseline formats
- ❌ Not waiting for CPU to stabilize

---

## Quick Reference: What Changed

| Aspect | Old | New | Reason |
|--------|-----|-----|--------|
| **Iterations** | `None` (auto) | `5` (fixed) | Auto-scaling caused 17-105% variance |
| **Samples** | `200` | `100,000` | More statistical power |
| **Comparison** | p90 | mean | Better with high sample counts |
| **Variance** | 17-105% | 0-3% | 20× improvement |
| **Runtime** | ~22s | ~70s | 3× slower but reliable |

---

## Files Requiring Updates

### Core Changes:
- ✅ `simplebench-runtime/src/config.rs` - Defaults, remove auto-scaling
- ✅ `simplebench-runtime/src/measurement.rs` - Remove estimate_iterations
- ✅ `simplebench-runtime/src/lib.rs` - Add mean, simplify runner
- ✅ `simplebench-runtime/src/baseline.rs` - No changes (auto-serializes mean)
- ✅ `simplebench-runtime/src/output.rs` - Format mean prominently
- ✅ `cargo-simplebench/src/runner_gen.rs` - Use streaming function

### Documentation:
- ✅ `CLAUDE.md` - Update measurement strategy
- ✅ `.claude/phase1_implementation_plan_v2.md` - Implementation guide
- ⬜ `CHANGELOG.md` - Breaking changes (when ready)

---

## Success Checklist

Before considering Phase 1 complete:

- [ ] All unit tests pass
- [ ] `test-workspace/.benches/` deleted
- [ ] Integration test runs successfully
- [ ] 5 repeated runs show <3% variance
- [ ] Mean displayed as primary metric
- [ ] No auto-scaling code remains
- [ ] Defaults are 5×100,000
- [ ] Results stream as benchmarks complete
- [ ] Documentation updated

---

## Remember for Future Phases

1. **Always delete baselines** when testing configuration changes
2. **Breaking changes OK** - don't hold back improvements
3. **Variance target: <3%** for all benchmarks
4. **Test on clean system** - no background load
5. **Document runtime trade-offs** - users need to know 3× slower

---

**Reference Documents:**
- Full analysis: `.claude/phase1_variance_review_final.md`
- Implementation plan: `.claude/phase1_implementation_plan_v2.md`
- Original research: `.claude/variance_research.md`
