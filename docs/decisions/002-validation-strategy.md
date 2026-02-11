# Validation Strategy

**Status**: Design Phase  
**Related To**: Testing Framework, Correctness Guarantees

## The Problem (from V1)

In V1, we constantly questioned:
- "Is the P&L calculation correct?"
- "Why are results different than expected?"
- "Did we introduce a bug in the last change?"

Every study required custom debug scripts. This is unsustainable.

## Goals for V2

1. **Confidence**: Know the code is correct without manual verification
2. **Debugging**: When something looks wrong, quickly find the cause
3. **Regression**: Changes don't break existing functionality
4. **Auditability**: Can explain any result with full traceability

## Validation Layers

### Layer 1: Unit Tests

**Every function tested in isolation:**

```rust
#[test]
fn test_black_76_call_pricing() {
    let option = FuturesOption {
        underlying: 62.0,
        strike: 62.0,
        dte: 1.0 / 365.0,
        iv: 0.30,
        risk_free_rate: 0.04,
        option_type: OptionType::Call,
    };
    
    let price = black_76_price(&option);
    
    // Known value from verified calculation
    assert!((price - 0.4466).abs() < 0.0001);
}
```

**Coverage targets:**
- 100% of pricing functions
- 100% of P&L calculation paths
- 100% of roll trigger conditions
- All edge cases (expiry, zero DTE, weekends)

---

### Layer 2: Property-Based Tests

**Invariant checks that must ALWAYS hold:**

```rust
#[test]
fn pnl_equals_credits_minus_debits_minus_commissions() {
    // Generate random simulations
    let sim = generate_random_simulation();
    
    let calculated_pnl = sim.total_pnl();
    let expected_pnl = sim.total_credits() - sim.total_debits() - sim.total_commissions();
    
    assert_eq!(calculated_pnl, expected_pnl);
}

#[test]
fn short_option_credit_is_positive() {
    // For short options, entry should always be credit (positive)
    for leg in sim.short_legs() {
        assert!(leg.entry_credit > 0.0);
    }
}

#[test]
fn roll_count_equals_opens_minus_closes() {
    assert_eq!(sim.roll_count, sim.open_count - sim.close_count);
}
```

**Properties to verify:**
- Accounting invariants (debits = credits - pnl - commissions)
- No negative commissions
- No trades outside market hours
- Position notional >= 0
- DTE always non-negative

---

### Layer 3: Golden Tests (Reference Simulations)

**10-20 hand-verified simulations with known outcomes:**

```
tests/golden/
├── sim_001_flat_market.yaml      # Price flat, known theta decay
├── sim_002_trending_up.yaml      # Strong uptrend, call ITM
├── sim_003_trending_down.yaml    # Strong downtrend, put ITM
├── sim_004_whipsaw.yaml          # Price oscillates
├── sim_005_gap_up.yaml           # Overnight gap
├── sim_006_weekend_roll.yaml     # Roll across weekend
├── sim_007_profit_target.yaml    # Profit target hit
├── sim_008_dte_trigger.yaml      # DTE threshold roll
├── sim_009_time_roll.yaml        # Time-based roll
└── sim_010_intraday_recenter.yaml # Intraday logic
```

Each golden test:
1. Fixed seed, fixed parameters
2. Hand-calculated expected results
3. Full event log for verification
4. CI runs these on every commit

**If golden tests fail, the build fails.**

---

### Layer 4: Differential Testing

**Compare V1 and V2 on same inputs:**

```python
# Run same simulation in both versions
v1_result = run_v1_simulation(seed=123, params=params)
v2_result = run_v2_simulation(seed=123, params=params)

# Results should match within tolerance
assert abs(v1_result.pnl - v2_result.pnl) < 0.01
assert v1_result.roll_count == v2_result.roll_count
```

**Purpose:** Verify V2 reproduces V1's correct behavior (catches regressions)

---

### Layer 5: Simulation Replay

**Every simulation produces an event log that can be replayed:**

```json
{
  "simulation_id": "sim_abc123",
  "events": [
    {"time": "2024-01-01T09:30:00", "type": "ENTRY", "leg": 0, "strike": 62.0, "credit": 450.0},
    {"time": "2024-01-01T14:00:00", "type": "ROLL", "leg": 0, "old_strike": 62.0, "new_strike": 62.5, "pnl": 230.0},
    // ...
  ]
}
```

**Capabilities:**
- Replay events to verify accounting
- Export to CSV for Excel analysis
- Visualize in notebook
- Pause at any event and inspect state

---

### Layer 6: Monte Carlo Convergence

**Statistical sanity checks:**

```rust
#[test]
fn monte_carlo_mean_converges() {
    // Run 100, 1000, 10000 sims
    // Mean P&L should converge (not keep changing wildly)
    let mean_100 = run_mc(100).mean_pnl();
    let mean_1000 = run_mc(1000).mean_pnl();
    let mean_10000 = run_mc(10000).mean_pnl();
    
    // Standard error should decrease
    assert!(std_err(mean_10000) < std_err(mean_1000));
    assert!(std_err(mean_1000) < std_err(mean_100));
}
```

---

## Validation Strategy Decision Matrix

| Approach | Catches | Runtime | Maintenance |
|----------|---------|---------|-------------|
| Unit tests | Logic errors | Fast | Low |
| Property tests | Invariant violations | Medium | Low |
| Golden tests | Regressions | Medium | Medium |
| Differential | Port errors | Slow | Medium |
| Replay | Debugging | N/A | Low |
| MC convergence | Statistical errors | Slow | Low |

**Recommendation:** Implement ALL layers.

## Implementation Priority

**Phase 1 (Foundation):**
1. Unit tests for pricing (Black-76)
2. Unit tests for accounting
3. Property tests for invariants

**Phase 2 (Simulation):**
4. Golden tests (10 reference sims)
5. Replay system

**Phase 3 (Validation):**
6. Differential testing (V1 vs V2)
7. MC convergence tests

---

## Questions for Stefan

1. **Tolerance for false positives:** Should tests fail on any numerical difference, or accept small epsilon (0.01%)?
2. **CI integration:** Run full test suite on every commit, or nightly?
3. **Golden test maintenance:** Who updates expected values when intentional changes happen?
4. **Performance:** Full MC test (10,000 sims) takes ~5 min — acceptable for CI?

---

**Next Steps:**
1. Review and approve strategy
2. Create ADR documenting decision
3. Implement test framework in chosen language
