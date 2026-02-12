# Trading Simulator V2 - Configuration Reference

**Last Updated:** 2026-02-12
**Version:** Phase 5 (Post Delta-Based Strikes)

This document serves as the authoritative reference for all configuration options, parameters, and rules in Trading Simulator V2. **Update this file whenever adding or modifying features.**

---

## Table of Contents

1. [YAML Configuration Structure](#yaml-configuration-structure)
2. [Simulation Parameters](#simulation-parameters)
3. [Strategy Configuration](#strategy-configuration)
4. [Strike Configuration](#strike-configuration)
5. [Roll Triggers](#roll-triggers)
6. [Product Configuration](#product-configuration)
7. [Examples](#examples)
8. [Implementation Notes](#implementation-notes)

---

## YAML Configuration Structure

```yaml
simulation:
  # See "Simulation Parameters" section

strategy:
  # See "Strategy Configuration" section

product:
  # See "Product Configuration" section

strike_config:
  # See "Strike Configuration" section
```

---

## Simulation Parameters

### `days` (required)
- **Type:** Integer
- **Description:** Number of trading days to simulate
- **Range:** 1 to 10,000
- **Example:** `days: 30`
- **Notes:** Skips weekends automatically based on synthetic calendar

### `initial_price` (required)
- **Type:** Float
- **Description:** Starting price for the underlying
- **Example:** `initial_price: 75.0` (for /CL at $75/barrel)
- **Notes:** GBM generator starts from this price

### `drift` (optional, default: 0.0)
- **Type:** Float
- **Description:** Annual expected return (μ)
- **Example:** `drift: 0.05` for 5% annual drift
- **Notes:** 
  - 0.0 = no directional bias (random walk)
  - Positive = upward trend
  - Negative = downward trend
  - Rarely used for short-term options (overwhelmed by volatility)

### `volatility` (required)
- **Type:** Float
- **Description:** Annualized volatility (σ)
- **Example:** `volatility: 0.30` for 30% annual vol
- **Range:** Must be > 0
- **Notes:** 
  - Typical values: 0.20-0.50 for most underlyings
  - Oil (/CL) often trades 25-35%
  - Higher vol = higher option premiums

### `seed` (required)
- **Type:** Integer (u64)
- **Description:** Random seed for GBM reproducibility
- **Example:** `seed: 42`
- **Notes:**
  - Same seed = identical price path
  - Essential for comparing strategies
  - Change seed to run Monte Carlo batches

### `risk_free_rate` (optional, default: 0.05)
- **Type:** Float
- **Description:** Annual risk-free rate for discounting
- **Example:** `risk_free_rate: 0.05` for 5%
- **Notes:**
  - Used in Black-76 pricing formula
  - Minimal impact on short-dated options (1DTE)
  - More significant for longer DTE (70DTE protection)

### `volatility_risk_premium` (optional, default: 0.0)
- **Type:** Float
- **Description:** Volatility Risk Premium - the edge option sellers capture
- **Example:** `volatility_risk_premium: 0.05` for 5% VRP
- **Formula:** `Implied Vol = Realized Vol + VRP`
- **Notes:**
  - **CRITICAL for option sellers** - this is where profit comes from
  - Realized vol drives price movements (GBM simulation)
  - Implied vol (realized + VRP) drives option prices
  - Example: 30% realized + 5% VRP = 35% implied
  - Historical VRP is typically 2-5% for most underlyings
  - Higher VRP = higher option premiums = more income for sellers
  - Set to 0.0 to disable (options priced at realized vol)

### `contract_multiplier` (optional, default: 1000.0)
- **Type:** Float
- **Description:** Number of units per contract
- **Example:** `contract_multiplier: 1000` for /CL (1000 barrels)
- **Common Values:**
  - `/CL` (Oil): 1000 barrels
  - `/ES` (E-mini S&P): $50 per point (but options on futures = multiplier)
  - Stocks: 100 shares
- **Notes:** Affects P&L calculations - premium × multiplier = dollar P&L

---

## Strategy Configuration

### `strategy_type` (required)
- **Type:** String
- **Description:** Type of options strategy
- **Valid Values:**
  - `"straddle"` - ATM put + ATM call (same strike)
  - `"strangle"` - OTM put + OTM call (different strikes)
  - `"iron_condor"` - (future implementation)
- **Example:** `strategy_type: "straddle"`
- **Notes:** Determines how strikes are selected

### `entry_dte` (required)
- **Type:** Integer
- **Description:** Days to expiration when position is opened
- **Example:** `entry_dte: 1` for 1DTE strategy
- **Common Values:**
  - `1` - 1DTE (daily rolling)
  - `0` - 0DTE (same day expiry, e.g., SPX)
  - `70` - Long protection (roll at 28 DTE)
- **Notes:** Calendar calculates actual expiration day from this

### `entry_time` (optional, default: "15:00")
- **Type:** String (HH:MM format)
- **Description:** Time to open new positions
- **Example:** `entry_time: "15:00"`
- **Notes:** 
  - First position opens at this time
  - Subsequent rolls use `roll_time`
  - NY time (matches /CL trading hours)

### `roll_time` (optional, default: "14:00")
- **Type:** String (HH:MM format)
- **Description:** Time to roll positions
- **Example:** `roll_time: "14:00"`
- **Notes:**
  - 1 hour before typical 14:30 expiry
  - Allows time to get filled
  - Must be before `entry_time` for 1DTE logic

### `strike_selection` (optional, default: "ATM")
- **Type:** String
- **Description:** How to select strikes
- **Valid Values:**
  - `"ATM"` - At-the-money (closest strike to current price)
  - `"OTM"` - Out-of-the-money (current price ± offset)
  - `"delta_put_XX"` - Put strike closest to XX delta (e.g., "delta_put_16")
  - `"delta_call_XX"` - Call strike closest to XX delta (e.g., "delta_call_30")
- **Example:** `strike_selection: "delta_put_16"`
- **Notes:**
  - Delta-based searches strikes ±20 ticks from ATM
  - Finds strike with delta closest to target
  - For puts: target is negative (e.g., -0.16 for 16 delta)
  - For calls: target is positive (e.g., 0.30 for 30 delta)

### `strike_offset` (optional, default: 0.0)
- **Type:** Float
- **Description:** Points OTM when using "OTM" selection
- **Example:** `strike_offset: 3.0` for 3 points OTM
- **Notes:**
  - Only used when `strike_selection: "OTM"`
  - Put strike = ATM - offset
  - Call strike = ATM + offset
  - Automatically rounded to valid tick size

### `roll_triggers` (optional)
- **Type:** Array of trigger objects
- **Description:** Conditions that trigger position rolls
- **Notes:** Currently only time-based is implemented
- **Future:** Profit target, DTE threshold, price move, delta threshold

#### Roll Trigger Format:
```yaml
roll_triggers:
  - trigger_type: "time"
    value: 14.0          # 14:00
    legs: "both"         # "both", "put", "call"
  - trigger_type: "profit_target"
    value: 0.50          # 50% of max profit
    legs: "both"
  - trigger_type: "dte"
    value: 28.0          # Roll when DTE <= 28
    legs: "long"         # Only for long protection
```

---

## Strike Configuration

### `tick_size` (optional, default: 0.25)
- **Type:** Float
- **Description:** Minimum increment between strikes
- **Example:** `tick_size: 0.25`
- **Common Values:**
  - `/CL` (Oil): 0.25
  - `SPY`: 1.0
  - `SPX`: 5.0
  - Individual stocks: Varies (often 1.0, 2.5, 5.0)
- **Notes:**
  - All strikes are rounded to this increment
  - Price $74.66 with 0.25 tick → $74.75
  - Price $233.33 with 1.0 tick → $233.00

### `roll_type` (optional, default: "recenter")
- **Type:** String
- **Description:** How to select strikes when rolling
- **Valid Values:**
  - `"recenter"` - New ATM strike based on current price
  - `"same_strikes"` - Keep same strikes as previous position
- **Example:** `roll_type: "recenter"`
- **Notes:**
  - "recenter" = traditional ATM rolling
  - "same_strikes" = keeps strikes fixed (may become ITM/OTM)
  - "same_strikes" enables per-leg rolling:
    - Tested side rolls to new ATM
    - Untested side keeps old strike → becomes inverted

---

## Roll Triggers

### Current Implementation

Only `trigger_type: "time"` is currently implemented.

### Planned Implementations

#### 1. Profit Target (`trigger_type: "profit_target"`)
- **Value:** Percentage of max profit (0.0 to 1.0)
- **Example:** `value: 0.50` for 50% profit target
- **Logic:** Roll when unrealized P&L >= target × max profit
- **Notes:** 
  - Max profit = premium received (for shorts)
  - V1 findings: PT-0 best for 1DTE straddles, PT14% for long protection

#### 2. DTE Threshold (`trigger_type: "dte"`)
- **Value:** Days to expiration threshold
- **Example:** `value: 28.0` for 28 DTE
- **Logic:** Roll when remaining DTE <= value
- **Notes:**
  - Used for long protection (roll at 28 DTE from 70 DTE entry)
  - Prevents holding through accelerating theta decay

#### 3. Price Move (`trigger_type: "price_move"`)
- **Value:** Points moved from entry
- **Example:** `value: 5.0` for 5-point move
- **Logic:** Roll when |current_price - entry_price| >= value
- **Notes:** Alternative to profit-based recentering

#### 4. Delta Threshold (`trigger_type: "delta_threshold"`)
- **Value:** Delta value (0.0 to 1.0)
- **Example:** `value: 0.30` for 30 delta
- **Logic:** Roll when option delta exceeds threshold
- **Notes:** Keep positions within desired delta range

---

## Product Configuration

### `symbol` (required)
- **Type:** String
- **Description:** Product symbol
- **Example:** `symbol: "/CL"`
- **Common Values:** `/CL`, `/ES`, `/NQ`, `SPX`, `SPY`

### `tick_size` (required)
- **Type:** Float
- **Description:** Minimum price increment for underlying
- **Example:** `tick_size: 0.01`
- **Notes:** Different from strike tick size

### `point_value` (required)
- **Type:** Float
- **Description:** Dollar value per point of underlying
- **Example:** `point_value: 1000.0` for /CL
- **Notes:** Used for futures P&L calculations

### `trading_hours` (required)

#### `open` (required)
- **Type:** String (HH:MM)
- **Example:** `open: "09:00"`

#### `close` (required)
- **Type:** String (HH:MM)
- **Example:** `close: "17:00"`

#### `option_expiry` (required)
- **Type:** String (HH:MM)
- **Example:** `option_expiry: "14:30"`
- **Notes:** When options expire (typically 14:30 for /CL)

---

## Examples

### 1. Basic 1DTE Straddle (/CL)
```yaml
simulation:
  days: 30
  initial_price: 75.0
  drift: 0.0
  volatility: 0.30
  seed: 42
  risk_free_rate: 0.05
  contract_multiplier: 1000

strategy:
  strategy_type: straddle
  entry_dte: 1
  entry_time: "15:00"
  roll_time: "14:00"
  strike_selection: ATM
  strike_offset: 0.0
  
  roll_triggers:
    - trigger_type: time
      value: 14.0
      legs: both

strike_config:
  tick_size: 0.25
  roll_type: recenter

product:
  symbol: "/CL"
  tick_size: 0.01
  point_value: 1000.0
  trading_hours:
    open: "09:00"
    close: "17:00"
    option_expiry: "14:30"
```

### 2. Long Protection with PT14% Recentering
```yaml
simulation:
  days: 200
  initial_price: 75.0
  drift: 0.0
  volatility: 0.30
  seed: 42

strategy:
  strategy_type: strangle
  entry_dte: 70
  entry_time: "15:00"
  roll_time: "14:00"
  strike_selection: OTM
  strike_offset: 3.0
  
  roll_triggers:
    - trigger_type: dte
      value: 28.0
      legs: long
    - trigger_type: profit_target
      value: 0.14
      legs: long

strike_config:
  tick_size: 0.25
  roll_type: recenter
```

### 3. Delta-Based Strangle (16/30)
```yaml
strategy:
  strategy_type: strangle
  entry_dte: 1
  strike_selection: delta_put_16  # Put at 16 delta
  # Call would need separate config or custom logic
  
strike_config:
  tick_size: 0.25
  roll_type: recenter
```

### 4. Per-Leg Rolling (Inverted Strangle)
```yaml
strategy:
  strategy_type: strangle
  entry_dte: 1
  
  roll_triggers:
    - trigger_type: profit_target
      value: 0.50
      legs: tested    # Roll tested side to ATM
    - trigger_type: dte
      value: 0.5
      legs: both      # Roll both at expiration

strike_config:
  roll_type: same_strikes  # Keep untested leg's strike
```

---

## Implementation Notes

### 1. Synthetic Calendar
- Day 0 = Monday, January 1, Year 0
- Trading days: Monday-Friday only
- No holidays (deterministic for testing)
- Weekend handling: Friday roll → Monday expiration (3 calendar days, 1 trading day)

### 2. Price Generation (GBM)
```
dS = μS dt + σS dW
Discretized: S(t+1) = S(t) × exp((μ - 0.5σ²)dt + σ√dt × Z)
```
- dt = 1/252 (one trading day in years)
- Z = standard normal random variable
- Same seed = identical price path

### 3. Black-76 Pricing
Used for futures options (/CL):
```
Call = e^(-rT) × [F × N(d1) - K × N(d2)]
Put  = e^(-rT) × [K × N(-d2) - F × N(-d1)]

d1 = [ln(F/K) + (σ²/2)T] / (σ√T)
d2 = d1 - σ√T
```
- F = futures price
- K = strike
- T = time to expiry (years)
- r = risk-free rate
- σ = volatility

### 4. Greeks Calculations
- **Delta:** ∂Price/∂Underlying (hedge ratio)
- **Gamma:** ∂Delta/∂Underlying (acceleration)
- **Theta:** ∂Price/∂Time (time decay, per day)
- **Vega:** ∂Price/∂Volatility (per 1% vol change)

### 5. P&L Calculation
```
Position P&L = (Entry Premium - Exit Premium) × Contract Multiplier

Example:
- Entry premium: $1.13 per barrel
- Exit premium: $0.50 per barrel (option expired ITM)
- P&L = ($1.13 - $0.50) × 1000 = $630 profit
```

### 6. Strike Rounding
```rust
strike = round(price / tick_size) × tick_size

Examples:
- Price $74.66, tick 0.25 → $74.75
- Price $233.33, tick 1.0 → $233.00
- Price $233.33, tick 5.0 → $235.00
```

### 7. Delta-Based Strike Search
```rust
for strike in (ATM - 20 ticks) to (ATM + 20 ticks):
    delta = calculate_delta(strike)
    if |delta - target| < best_diff:
        best_strike = strike
        best_diff = |delta - target|
```

### 8. Roll Logic
1. Check if current day == expiration day
2. If yes: Close position at 14:00
3. Calculate P&L
4. Open new position based on roll_type:
   - "recenter": Use current price for new strikes
   - "same_strikes": Use previous strikes
5. Record both close and open events

---

## Changelog

### 2026-02-12 (Phase 5b - VRP)
- Added Volatility Risk Premium (VRP) parameter
- Price path uses realized vol, options priced at implied vol
- Implied Vol = Realized Vol + VRP
- Critical for option seller edge modeling

### 2026-02-12 (Phase 5)
- Added delta-based strike selection
- Added strike tick size configuration
- Added roll type (recenter/same_strikes)
- Updated P&L display with per-barrel and total values

### 2026-02-12 (Phase 4)
- Added YAML configuration system
- Separated config from code

### 2026-02-12 (Phase 3)
- Added Black-76 pricing
- Added Greeks calculation
- Added P&L tracking
- Added contract multiplier

### 2026-02-12 (Phase 2)
- Added GBM price generator
- Added reproducible seeds

### 2026-02-12 (Phase 1)
- Initial implementation
- Synthetic calendar
- Event sourcing architecture
- Basic 1DTE straddle

---

## TODO / Future Enhancements

1. **Implement roll triggers:**
   - [ ] Profit target (50%, 14%, etc.)
   - [ ] DTE threshold (28 DTE for longs)
   - [ ] Price move triggers
   - [ ] Delta threshold

2. **Long protection legs:**
   - [ ] 70 DTE put entry
   - [ ] Separate roll logic for longs
   - [ ] Combined short + long positions

3. **Monte Carlo framework:**
   - [ ] Batch simulation runner
   - [ ] Statistics aggregation
   - [ ] Parallel execution

4. **Additional strike selection:**
   - [ ] Separate put/call delta targets
   - [ ] Percentage OTM (e.g., 2% below)
   - [ ] Fixed dollar amounts

5. **UI/Tauri:**
   - [ ] Desktop application
   - [ ] Real-time visualization
   - [ ] Parameter sliders

---

**Document Maintainer:** Basil (AI Assistant)  
**Review Schedule:** Update after each phase completion  
**Chroma Index:** Yes (for semantic search)
