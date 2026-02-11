# Universal Strategy Configuration System

**Status**: Design Phase  
**Owner**: Stefan & Basil  
**Purpose**: Define ANY trading strategy — from simple one-off trades to complex continuous rolling

## The Core Insight

Every strategy has the same fundamental structure:

```
Position Entry → [Hold Period] → Exit OR Roll → Repeat
```

The differences are:
- **Simple trades**: Exit is final (take profit, stop loss, or expiration)
- **Rolling strategies**: Exit triggers a new entry (continuous)

## Strategy Types

### Type 1: Simple Trade (One-Off)

**Example**: Sell 16-delta put, 45 DTE, take profit 50%, stop loss 200%

```yaml
strategy:
  name: "Simple Put Sale"
  type: simple_trade
  
  entry:
    legs:
      - side: short
        instrument: put
        dte: 45
        strike_selection:
          type: delta
          target: 0.16
    
  exit_conditions:
    # Any of these triggers exit
    - type: profit_target
      pct: 0.50
    
    - type: stop_loss
      pct: 2.00  # 200%
    
    - type: dte_threshold
      dte: 7  # Exit at 7 DTE if still open
    
    - type: expiration  # Always exit at expiration
  
  # Optional: salvage logic
  if_not_profitable_at:
    dte: 7
    action: roll  # Roll to next month for more credit
    to_dte: 45
```

### Type 2: Continuous Rolling Strategy

**Example**: Oil 1DTE straddle, roll daily at 14:00

```yaml
strategy:
  name: "Oil 1DTE Straddle"
  type: continuous_roll
  
  entry:
    legs:
      - side: short
        instrument: put
        dte: 1
        strike_selection:
          type: atm
      
      - side: short
        instrument: call
        dte: 1
        strike_selection:
          type: atm
    
    # Position-level (both legs together)
    position_profit_target:
      pct: 0.50
      action: close_both  # Close both legs together
  
  roll_conditions:
    # Roll triggers (any can trigger)
    - type: time_of_day
      time: "14:00"
    
    - type: dte_threshold
      dte: 0
    
    # Roll execution
    roll_mode: synchronized  # Both legs roll together
    to_dte: 1
    
    # Strike selection on roll
    new_strike:
      type: atm  # Recenter to new ATM
```

### Type 3: Mixed/Hybrid

**Example**: Strangle with independent leg management

```yaml
strategy:
  name: "Strangle with Independent Management"
  type: continuous_roll
  
  entry:
    legs:
      - leg_id: put_leg
        side: short
        instrument: put
        dte: 1
        strike_selection:
          type: otm_points
          offset: 2.0
        
        # Leg-specific triggers
        roll_conditions:
          - type: profit_target
            pct: 0.50
          
          - type: time_of_day
            time: "14:00"
        
        roll_mode: independent
      
      - leg_id: call_leg
        side: short
        instrument: call
        dte: 1
        strike_selection:
          type: otm_points
          offset: 2.0
        
        roll_conditions:
          - type: profit_target
            pct: 0.30  # Different target!
          
          - type: price_move
            points: 3.0  # Recenter if price moves 3pts
        
        roll_mode: independent
```

## Configuration Schema

### Entry Configuration

```rust
struct EntryConfig {
    legs: Vec<LegConfig>,
    position_level: Option<PositionLevelConfig>,
}

struct LegConfig {
    leg_id: String,
    side: Side,  // Long or Short
    instrument: InstrumentType,  // Call, Put, Underlying
    
    // Timing
    dte: DteSelection,
    
    // Strike selection (product-specific)
    strike_selection: StrikeSelection,
}

enum DteSelection {
    Fixed(u8),           // Exact DTE (e.g., 1, 45, 70)
    Range { min: u8, max: u8 },  // Choose best within range
}

enum StrikeSelection {
    ATM,                          // At-the-money
    OTM { offset: f64 },          // OTM by X points
    ITM { offset: f64 },          // ITM by X points
    Delta { target: f64 },        // Target delta (e.g., 0.16)
    Percentage { pct: f64 },      // % of underlying (e.g., 0.98 for 2% OTM put)
    Fixed(f64),                   // Specific strike
}
```

### Exit/Roll Conditions

```rust
enum ExitCondition {
    // Profit/Loss based
    ProfitTarget { pct: f64 },
    StopLoss { pct: f64 },
    
    // Time based
    DteThreshold { dte: f64 },
    TimeOfDay { time: Time },
    DaysInTrade { days: u32 },
    
    // Price based
    PriceMove { 
        points: Option<f64>,
        pct: Option<f64>,
        from: PriceReference,  // Entry, last_roll, daily_open
    },
    
    // Greeks based (future)
    DeltaThreshold { delta: f64 },
    ThetaThreshold { theta: f64 },
    
    // Technical (future)
    TechnicalIndicator { indicator: String, condition: String },
    
    // Always exit at expiration
    Expiration,
    
    // Manual/user override
    Manual,
}

enum PriceReference {
    Entry,           // Price when position opened
    LastRoll,        // Price at last roll
    DailyOpen,       // Opening price today
    HighSinceEntry,  // Highest price since entry
    LowSinceEntry,   // Lowest price since entry
}
```

### Roll Configuration

```rust
struct RollConfig {
    mode: RollMode,
    triggers: Vec<ExitCondition>,  // Any of these triggers roll
    new_position: NewPositionConfig,
    
    // Cooldowns
    min_time_between_rolls: Option<Duration>,
    max_rolls_per_day: Option<u32>,
}

enum RollMode {
    Independent,   // Each leg rolls on its own triggers
    Synchronized,  // All legs roll together (use most restrictive trigger)
    LeaderFollower { leader: String },  // Leader leg triggers, followers roll too
}

struct NewPositionConfig {
    to_dte: DteSelection,
    strike_selection: StrikeSelection,
    maintain_spread: bool,  // For spreads: maintain width
}
```

### If-Not-Profitable Logic (Salvage)

```rust
struct SalvageConfig {
    condition: ExitCondition,
    action: SalvageAction,
}

enum SalvageAction {
    Roll { config: NewPositionConfig },
    Close,      // Take the loss
    Hold,       // Keep until expiration
    AddHedge,   // Add protective leg
}
```

## Product-Specific Adaptations

### /CL (Oil Futures)

```yaml
product_config:
  symbol: "/CL"
  tick_size: 0.01
  point_value: 1000
  strike_increment: 0.25
  
  # Trading calendar
  trading_hours:
    sunday: "18:00-17:00"  # Next day
    monday_thursday: "09:00-17:00"
    friday: "09:00-17:00"
  
  # Expiration
  expiration:
    day: friday_before_last_trading_day
    time: "14:30"
```

### SPX (Index Options)

```yaml
product_config:
  symbol: "SPX"
  tick_size: 0.01
  point_value: 100  # $100 per point
  strike_increment: 5.0  # SPX strikes in $5 increments
  
  # SPX has Monday, Wednesday, Friday expirations
  available_expirations: [monday, wednesday, friday]
  
  # 0DTE trading
  supports_0dte: true
```

### Custom Stock

```yaml
product_config:
  symbol: "AAPL"
  tick_size: 0.01
  point_value: 100  # Options are 100 shares
  strike_increment: variable  # Depends on stock price
  
  # Weekly and monthly expirations
  available_expirations: [weekly, monthly]
```

## Example Strategies in Full

### Example 1: Oil 1DTE Straddle (Current)

```yaml
name: "Oil 1DTE Short Straddle"
type: continuous_roll

entry:
  legs:
    - leg_id: put
      side: short
      instrument: put
      dte: 1
      strike_selection: { type: atm }
    
    - leg_id: call
      side: short
      instrument: call
      dte: 1
      strike_selection: { type: atm }

roll_conditions:
  mode: synchronized
  
  triggers:
    - type: time_of_day
      time: "14:00"
    - type: dte_threshold
      dte: 0
  
  new_position:
    to_dte: 1
    strike_selection: { type: atm }
  
  cooldowns:
    min_time_between_rolls: 1 hour
    max_rolls_per_day: 1
```

### Example 2: 45 DTE Put (Simple Trade)

```yaml
name: "45 DTE Put Sale"
type: simple_trade

entry:
  legs:
    - side: short
      instrument: put
      dte: 45
      strike_selection: 
        type: delta
        target: 0.16

exit_conditions:
  any_of:  # Any triggers exit
    - type: profit_target
      pct: 0.50
    
    - type: stop_loss
      pct: 2.00
    
    - type: dte_threshold
      dte: 7
    
    - type: expiration

salvage:
  if_not_profitable_at:
    condition: { type: dte_threshold, dte: 7 }
    action:
      type: roll
      to_dte: 45
      strike_selection: { type: delta, target: 0.16 }
```

### Example 3: 0DTE SPX Strangle (Intraday)

```yaml
name: "SPX 0DTE Strangle"
type: continuous_roll

entry:
  legs:
    - leg_id: put
      side: short
      instrument: put
      dte: 0  # Same day expiration
      strike_selection:
        type: otm_points
        offset: 10.0  # 10 points OTM
    
    - leg_id: call
      side: short
      instrument: call
      dte: 0
      strike_selection:
        type: otm_points
        offset: 10.0

roll_conditions:
  mode: synchronized
  
  triggers:
    - type: time_of_day
      time: "15:30"  # Close before 16:00 expiration
  
  new_position:
    to_dte: 0  # Next day's 0DTE
    strike_selection: { type: atm }  # Recenter
```

### Example 4: Strangle with Recentering

```yaml
name: "Strangle with Price Recenter"
type: continuous_roll

entry:
  legs:
    - leg_id: put
      side: short
      instrument: put
      dte: 1
      strike_selection: { type: otm_points, offset: 2.0 }
    
    - leg_id: call
      side: short
      instrument: call
      dte: 1
      strike_selection: { type: otm_points, offset: 2.0 }

roll_conditions:
  mode: independent  # Legs roll independently!
  
  triggers:
    - type: profit_target
      pct: 0.50
    
    - type: price_move
      points: 3.0
      from: entry
    
    - type: time_of_day
      time: "14:00"
  
  new_position:
    to_dte: 1
    strike_selection: { type: atm }  # Recenter to new ATM
```

## Implementation Approach

### Phase 1: Core Framework

1. **Entry system** — Open positions with DTE and strike selection
2. **Simple exit** — Profit target, stop loss, expiration
3. **Basic roll** — Time-based and DTE-based
4. **Single product** — /CL only

### Phase 2: Advanced Features

1. **Independent leg management**
2. **Price-based recentering**
3. **Greek-based triggers**
4. **Multiple products** — /ES, SPX

### Phase 3: Full Flexibility

1. **Custom strategies via plugins**
2. **Multi-leg complex spreads**
3. **Portfolio-level management**
4. **Dynamic position sizing**

## Validation

Every strategy config must pass validation:

```rust
fn validate(config: &StrategyConfig) -> Result<(), ValidationError> {
    // Check: DTE is reasonable
    if config.entry.dte > 365 {
        return Err(ValidationError::DteTooHigh);
    }
    
    // Check: Stop loss > profit target (usually)
    if let (Some(sl), Some(pt)) = (config.stop_loss, config.profit_target) {
        if sl < pt {
            warn!("Stop loss ({}) is tighter than profit target ({})", sl, pt);
        }
    }
    
    // Check: Roll DTE >= exit DTE
    if let (Some(roll_dte), Some(exit_dte)) = (config.roll_dte, config.exit_dte) {
        if roll_dte > exit_dte {
            return Err(ValidationError::RollAfterExit);
        }
    }
    
    // Check: Leg IDs are unique
    let mut seen_ids = HashSet::new();
    for leg in &config.entry.legs {
        if !seen_ids.insert(&leg.leg_id) {
            return Err(ValidationError::DuplicateLegId);
        }
    }
    
    Ok(())
}
```

## Open Questions

1. **Salvage logic**: Should we support multiple salvage attempts (roll, then if still bad, close)?

2. **Partial exits**: Should we support closing 50% at profit target, let rest run?

3. **Scaling**: Should we support adding to position (scale in) or reducing (scale out)?

4. **Correlated legs**: How to handle spreads where legs must maintain fixed relationship?

5. **Multi-day**: Should we support strategies that hold overnight but not through weekend?

---

**Next Steps:**
1. Review this design
2. Confirm it covers your use cases
3. Identify any missing cases
4. Create ADR documenting final design

**Does this capture what you need? Any missing scenarios?**
