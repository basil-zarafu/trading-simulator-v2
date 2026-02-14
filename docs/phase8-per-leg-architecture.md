# Phase 8: Per-Leg Position Management Architecture

## Overview

This document describes the architecture for independent leg management in the trading simulator, supporting complex rolling strategies like "up and out", "tighten", and inverted position handling.

## Real-World Trading Logic (Stefan's Rules)

### Normal Flow (Day 0)
- Open straddle at ~2:00 PM (both legs 1 DTE)
- Strikes centered at ATM
- Both legs expire next day

### Intraday Adjustments (Next Day, Before 2:00 PM)

#### Case 1: Untested Side (Deep OTM, >90% Profit)
- **Trigger**: One leg shows 90%+ profit (untested, OTM)
- **Action**: TIGHTEN — roll closer to current price
- **Same expiration** — keep 1 DTE
- **Result**: Position becomes INVERTED (strikes different)

**Example**:
- Open: Put $75, Call $75
- Price moves to $76.50
- Put at 90% profit (untested)
- Roll put to $76.50 strike (tighten)
- Now: Put $76.50, Call $75 (inverted by 1.50 points)

#### Case 2: Tested Side (Deep ITM, >1 Point Intrinsic)
- **Trigger**: Leg is >1 point ITM
- **Action**: UP_AND_OUT (calls) or DOWN_AND_OUT (puts)
- **Strike shift**: +0.25 or +0.50 (up for calls, down for puts)
- **Extend DTE**: 2-3 days
- **Goal**: Convert intrinsic to time value

**Example** (Stefan's corrected explanation):
- Current: Short call @ $62.00
- Price: $63.50 (intrinsic = $1.50)
- Roll to: Call @ $62.50, exp 2-3 days out
- New call priced at ~$1.50 ($1.25 intrinsic + $0.25 extrinsic)
- If price reverts to $62.50: Keep full $1.50 credit

### End-of-Day Position Roll (2:00 PM)

#### Normal Case (No Special Conditions)
- Roll BOTH legs to next day
- Recenter both to new ATM
- Both legs now 1 DTE

#### Diverged Expirations Case
- After "up and out", legs have different expirations
- Put might be Day 6, Call might be Day 7
- Handle each leg independently based on its expiration

### Re-Syncing Inverted Positions

**Trigger**:
- Inverted spread <= 1.5 points
- Price ends up between the two strikes

**Action**:
- Recenter BOTH legs to ATM
- Same expiration (next day)
- Return to normal straddle

## Data Model

### Leg Structure

```rust
struct Leg {
    leg_id: LegId,              // "P" for put, "C" for call
    option_type: OptionType,    // Put or Call
    
    // Pricing
    strike: f64,
    entry_premium: f64,
    current_premium: f64,       // Mark-to-market
    
    // Lifecycle
    entry_timestamp: Timestamp,
    expiration_day: u32,        // Can differ from paired leg!
    
    // State
    status: LegStatus,
    greeks: Greeks,
    
    // History
    roll_history: Vec<RollEvent>,
}

enum LegStatus {
    Normal,      // Standard 1DTE leg
    Inverted,    // Rolled closer, same expiration
    Extended,    // "Up/out" or "down/out" — longer DTE
    Held,        // Tested >1pt ITM, keeping for next day
}

enum OptionType {
    Put,
    Call,
}
```

### Position Structure

```rust
struct Position {
    position_id: PositionId,
    legs: HashMap<LegId, Leg>,  // "P" -> put, "C" -> call
    created_at: Timestamp,
}

impl Position {
    fn is_inverted(&self) -> bool {
        let put_strike = self.legs.get("P").map(|l| l.strike);
        let call_strike = self.legs.get("C").map(|l| l.strike);
        match (put_strike, call_strike) {
            (Some(p), Some(c)) => p > c,  // Put strike > Call strike = inverted
            _ => false,
        }
    }
    
    fn inverted_spread(&self) -> Option<f64> {
        let put_strike = self.legs.get("P").map(|l| l.strike)?;
        let call_strike = self.legs.get("C").map(|l| l.strike)?;
        if put_strike > call_strike {
            Some(put_strike - call_strike)
        } else {
            None
        }
    }
}
```

### Roll Types

```rust
enum RollType {
    /// Move strike to new ATM (or offset), change expiration
    Recenter {
        to_strike: f64,
        to_expiration: u32,
    },
    
    /// Keep strike, extend expiration
    SameStrike {
        to_expiration: u32,
    },
    
    /// Move closer to price (untested side), same expiration
    Tighten {
        to_strike: f64,
    },
    
    /// Roll tested call up and out
    UpAndOut {
        strike_shift: f64,    // 0.25 or 0.50
        extend_dte: u32,      // 2 or 3 days
    },
    
    /// Roll tested put down and out
    DownAndOut {
        strike_shift: f64,    // 0.25 or 0.50
        extend_dte: u32,      // 2 or 3 days
    },
}

struct RollEvent {
    timestamp: Timestamp,
    roll_type: RollType,
    from_strike: f64,
    to_strike: f64,
    from_expiration: u32,
    to_expiration: u32,
    premium_closed: f64,
    premium_opened: f64,
    pnl: f64,
}
```

## Trigger System

### Trigger Conditions

```rust
enum TriggerCondition {
    /// Leg profit >= threshold (for untested side)
    ProfitThreshold { percent: f64 },
    
    /// Intrinsic value >= threshold (for tested side)
    IntrinsicThreshold { points: f64 },
    
    /// Time of day reached
    TimeOfDay { hour: u32, minute: u32 },
    
    /// DTE <= threshold
    DteThreshold { dte: f64 },
    
    /// Position is inverted with spread <= threshold
    InvertedResync { max_spread: f64 },
    
    /// Price between inverted strikes
    PriceBetweenStrikes,
}
```

### Decision Rules

```rust
struct RollRule {
    name: String,
    conditions: Vec<TriggerCondition>,
    action: RollAction,
    priority: u32,  // Higher = checked first
}

enum RollAction {
    RollLeg { leg_id: LegId, roll_type: RollType },
    RollAll { roll_type: RollType },
    Hold,  // Do nothing
}
```

### Default Rules (Stefan's Strategy)

```rust
vec![
    // Rule 1: Tighten untested side (90% profit)
    RollRule {
        name: "Tighten Untested".to_string(),
        conditions: vec![
            TriggerCondition::ProfitThreshold { percent: 90.0 },
        ],
        action: RollAction::RollLeg { 
            leg_id: LegId::from("P"),  // or "C", checked per leg
            roll_type: RollType::Tighten { to_strike: 0.0 },  // 0 = calculate at runtime
        },
        priority: 100,
    },
    
    // Rule 2: Up and out for tested calls (>1 point ITM)
    RollRule {
        name: "Up and Out".to_string(),
        conditions: vec![
            TriggerCondition::IntrinsicThreshold { points: 1.0 },
            // Implicit: must be a call
        ],
        action: RollAction::RollLeg {
            leg_id: LegId::from("C"),
            roll_type: RollType::UpAndOut { 
                strike_shift: 0.25, 
                extend_dte: 2 
            },
        },
        priority: 90,
    },
    
    // Rule 3: Down and out for tested puts (>1 point ITM)
    RollRule {
        name: "Down and Out".to_string(),
        conditions: vec![
            TriggerCondition::IntrinsicThreshold { points: 1.0 },
            // Implicit: must be a put
        ],
        action: RollAction::RollLeg {
            leg_id: LegId::from("P"),
            roll_type: RollType::DownAndOut { 
                strike_shift: 0.25, 
                extend_dte: 2 
            },
        },
        priority: 90,
    },
    
    // Rule 4: Re-sync inverted positions
    RollRule {
        name: "Resync Inverted".to_string(),
        conditions: vec![
            TriggerCondition::InvertedResync { max_spread: 1.5 },
            TriggerCondition::PriceBetweenStrikes,
        ],
        action: RollAction::RollAll {
            roll_type: RollType::Recenter { 
                to_strike: 0.0,  // Calculate ATM
                to_expiration: 0, // Calculate 1 DTE
            },
        },
        priority: 80,
    },
    
    // Rule 5: Normal end-of-day roll
    RollRule {
        name: "Daily Roll".to_string(),
        conditions: vec![
            TriggerCondition::TimeOfDay { hour: 14, minute: 0 },
            TriggerCondition::DteThreshold { dte: 1.0 },
        ],
        action: RollAction::RollAll {
            roll_type: RollType::Recenter { 
                to_strike: 0.0,
                to_expiration: 0,
            },
        },
        priority: 10,
    },
]
```

## Simulation Loop

```rust
fn simulate(config: &Config) {
    let calendar = TradingCalendar::new();
    let mut position: Option<Position> = None;
    
    for price_point in generate_price_path(&calendar, config) {
        let current_price = price_point.price;
        let timestamp = price_point.timestamp;
        
        // Phase 1: Check leg-level triggers (intraday)
        if let Some(ref mut pos) = position {
            for (leg_id, leg) in &mut pos.legs {
                if let Some(action) = check_leg_triggers(leg, current_price, timestamp) {
                    execute_roll(leg, action, current_price, timestamp);
                }
            }
        }
        
        // Phase 2: Check position-level triggers (daily/scheduled)
        if let Some(ref mut pos) = position {
            if let Some(action) = check_position_triggers(pos, current_price, timestamp) {
                match action {
                    RollAction::RollAll { roll_type } => {
                        for (leg_id, leg) in &mut pos.legs {
                            execute_roll(leg, roll_type, current_price, timestamp);
                        }
                    }
                    RollAction::RollLeg { leg_id, roll_type } => {
                        if let Some(leg) = pos.legs.get_mut(&leg_id) {
                            execute_roll(leg, roll_type, current_price, timestamp);
                        }
                    }
                    RollAction::Hold => {}
                }
            }
        }
        
        // Phase 3: Open new position if none
        if position.is_none() && should_open(timestamp) {
            position = Some(open_position(current_price, timestamp, config));
        }
        
        // Phase 4: Update leg marks (current premiums, greeks)
        if let Some(ref mut pos) = position {
            update_leg_marks(pos, current_price, timestamp);
        }
    }
}
```

## Configuration Format (YAML)

```yaml
strategy:
  strategy_type: straddle
  entry_dte: 1
  entry_time: "14:00"
  
  # Leg-level triggers (checked every 10 minutes)
  leg_triggers:
    put:
      - name: "Tighten Untested"
        condition: { profit_percent: 90 }
        action: { type: tighten }
        priority: 100
        
      - name: "Down and Out"
        condition: { intrinsic_points: 1.0 }
        action: { type: down_and_out, strike_shift: 0.25, extend_dte: 2 }
        priority: 90
        
    call:
      - name: "Tighten Untested"
        condition: { profit_percent: 90 }
        action: { type: tighten }
        priority: 100
        
      - name: "Up and Out"
        condition: { intrinsic_points: 1.0 }
        action: { type: up_and_out, strike_shift: 0.25, extend_dte: 2 }
        priority: 90
  
  # Position-level triggers (checked at specific times)
  position_triggers:
    - name: "Resync Inverted"
      condition: { inverted_spread_max: 1.5, price_between_strikes: true }
      action: { type: recenter_both }
      priority: 80
      
    - name: "Daily Roll"
      condition: { time: "14:00", dte_max: 1.0 }
      action: { type: recenter_both }
      priority: 10
```

## Logging Format

### Per-Leg Events

```
Day 4, 14:00 | Price $75.00
  [PUT]  OPENED #5 | Strike $75.00 | Premium $0.60 ($600)
  [CALL] OPENED #5 | Strike $75.00 | Premium $0.60 ($600)
  [COMBINED] Total Premium $1.20 ($1200)

Day 5, 10:30 | Price $76.50
  [PUT]  PROFIT 92% | TIGHTEN | Strike $75.00 → $76.50 | Same expiration
  [COMBINED] Now INVERTED: Put $76.50, Call $75.00 (spread 1.50)

Day 5, 14:00 | Price $76.75
  [CALL] DEEP ITM (1.75 points) | UP_AND_OUT | Strike $75.00 → $75.50 | Exp Day 5 → Day 7
  [PUT]  RECENTER | Strike $76.50 → $76.75 | Exp Day 5 → Day 6
  [COMBINED] Now DIVERGED: Put $76.75 (Day 6), Call $75.50 (Day 7)

Day 7, 12:00 | Price $76.25 (between strikes)
  [RESYNC TRIGGER] Inverted spread 1.25 <= 1.50, price between strikes
  [PUT]  RECENTER | Strike $76.75 → $76.25 | Exp Day 6 → Day 8
  [CALL] RECENTER | Strike $75.50 → $76.25 | Exp Day 7 → Day 8
  [COMBINED] Now NORMAL: Both at $76.25, both exp Day 8
```

## UI Implications

### New Display Elements Needed

1. **Leg-Level Detail View**
   - Show put and call as separate rows
   - Each leg: strike, expiration, current value, P&L, status
   
2. **Position State Indicator**
   - Normal / Inverted / Diverged badges
   - Spread width for inverted positions
   - Days to expiration for each leg
   
3. **Roll Decision Log**
   - Which leg triggered
   - What rule fired
   - Action taken
   - Before/after state

4. **Configuration Panel**
   - Per-leg trigger rules
   - Parameter inputs (profit %, intrinsic points, etc.)
   - Priority ordering

(Detailed UI design to be discussed with Stefan)

## Testing Parameters

The following should be Monte Carlo tested to find optimal values:

| Parameter | Test Range | Description |
|-----------|-----------|-------------|
| `profit_threshold` | 85%, 90%, 95% | When to tighten untested side |
| `intrinsic_threshold` | 0.75, 1.0, 1.25 | When to do up/down and out |
| `strike_shift` | 0.25, 0.50 | How much to move strike |
| `extend_dte` | 2, 3, 4 | Days to extend for up/down and out |
| `resync_spread` | 1.0, 1.5, 2.0 | When to re-sync inverted positions |

## Open Questions

1. Should we allow more than 2 legs per position? (for future strategies)
2. How to handle expiration day when legs have different expirations?
3. What if both legs trigger simultaneously (both >90% profit)?

---

*Document Version: 1.0*
*Last Updated: 2026-02-14*
*Next Step: UI Design Discussion*
