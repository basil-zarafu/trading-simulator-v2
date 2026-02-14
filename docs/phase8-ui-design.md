# Phase 8 UI Design Specification

**Based on Stefan's Requirements (2026-02-14)**

---

## 1. Left Panel Redesign

### Current
```
┌─ Configuration ─────────────┐
│ Strategy: [Straddle ▼]      │
│ Days: [120]                 │
│ Initial Price: [75.00]      │
│ Volatility: [0.30]          │
│ VRP: [0.05]                 │
│ Seed: [42]                  │
│                             │
│ [Run Simulation]            │
└─────────────────────────────┘
```

### New Design
```
┌─ Pairs Configuration ───────┐
│                             │
│ ┌─ Short Pair #1 ─────────┐ │
│ │ ☑ Active                │ │
│ │ Type: [Straddle ▼]      │ │
│ │   Offset: [0.00] points │ │
│ │ Entry DTE: [1 ▼]        │ │
│ │ Entry Time: [14:00]     │ │
│ │ Roll at DTE: [1 ▼]      │ │
│ │ New DTE: [1 ▼]          │ │
│ │                         │ │
│ │ [Configure Triggers ▼]  │ │
│ │   Profit ≥ [90] %       │ │
│ │   Intr ≥ [1.0] points   │ │
│ │   Up/Out shift: [0.25]  │ │
│ │   Extend: [2] days      │ │
│ └─────────────────────────┘ │
│                             │
│ ┌─ Long Pair #1 ──────────┐ │
│ │ ☑ Active                │ │
│ │ Type: [Straddle ▼]      │ │
│ │   Offset: [0.00] points │ │
│ │ Entry DTE: [70 ▼]       │ │
│ │ Entry Time: [14:00]     │ │
│ │ Roll at DTE: [28 ▼]     │ │
│ │ New DTE: [70 ▼]         │ │
│ │                         │ │
│ │ [Configure Triggers ▼]  │ │
│ └─────────────────────────┘ │
│                             │
│ [+ Add Short Pair]          │
│ [+ Add Long Pair]           │
│                             │
├─ Global Settings ───────────┤
│ Days (Calendar): [120]      │
│ Initial Price: [75.00]      │
│ Volatility: [0.30]          │
│ VRP: [0.05]                 │
│ Seed: [42]                  │
│                             │
│ [Run Simulation]            │
└─────────────────────────────┘
```

### Key Features
- **Offset field**: For strangles (0.25, 0.50, ... 2.00 points OTM)
- **Per-pair configuration**: Each pair has independent DTE, roll rules, triggers
- **Collapsible trigger config**: Hidden by default, expandable per pair
- **Add multiple pairs**: Can run short + long + additional pairs

---

## 2. Trade Log Redesign

### Layout
```
┌─ Trade Log ─────────────────────────────────────────────┐
│                                                          │
│ [All] [Puts] [Calls] [Opens] [Rolls] [Closes] [Export▼] │
│ ┌─ Events ────────────────────────────────────────────┐ │
│ │                                                     │ │
│ │ Day 0, 14:00  [OPEN]  [SHORT PUT #1]    $75.00    │ │
│ │                       [SHORT CALL #1]   $75.00    │ │
│ │                       Combined:        $1500      │ │
│ │                                                     │ │
│ │ Day 1, 10:30  [ROLL]  [SHORT PUT #1]  Tighten     │ │
│ │                       $75.00 → $76.50  (92% prof) │ │
│ │                                                     │ │
│ │ Day 1, 14:00  [ROLL]  [SHORT CALL #1] Up & Out    │ │
│ │                       $75.00 → $75.50  +2 days    │ │
│ │              [ROLL]  [SHORT PUT #1]   Recenter    │ │
│ │                       $76.50 → $76.75  Day 6      │ │
│ │                                                     │ │
│ │ Day 3, 12:00  [RESYNC] Position #1  Both legs     │ │
│ │                       Put: $76.75 → $76.25        │ │
│ │                       Call: $75.50 → $76.25       │ │
│ │                                                     │ │
│ └─────────────────────────────────────────────────────┘ │
│                                                          │
│ Showing 47 events  [◀ Prev] Page 1/3 [Next ▶]           │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

### Filter Tabs
- **All**: Every event in chronological order
- **Puts**: Only put leg events
- **Calls**: Only call leg events  
- **Opens**: Position open events
- **Rolls**: All roll events (tighten, up/down and out, recenter)
- **Closes**: Position close/final events
- **Export**: Dropdown with [CSV] [JSON] options

### CSV Export Format
```csv
Day,Time,Event Type,Leg,Position ID,From Strike,To Strike,From Exp,To Exp,Premium,Reason
0,14:00,OPEN,PUT,1,75.00,,1,,0.60,
0,14:00,OPEN,CALL,1,75.00,,1,,0.60,
1,10:30,ROLL,PUT,1,75.00,76.50,1,1,0.15,PROFIT_90
1,14:00,ROLL,CALL,1,75.00,75.50,1,3,1.50,UP_AND_OUT
1,14:00,ROLL,PUT,1,76.50,76.75,1,2,0.45,RECENTER
3,12:00,RESYNC,BOTH,1,76.75/75.50,76.25,2/3,4,1.20,SYNC_INVERTED
```

---

## 3. Roll History Visualization (Priority #1)

### On Price Chart
```
Price
  │
  │    ●── Roll Put (tighten)
  │   ╱
  │  ╱  ◆── Resync
  80│─●───── Roll Call (up & out)
  │ │
  │ │
 75│─┼───── Initial position
  │ │
  │
  └──────────────►
    Day 0  1  2  3

Legend:
  ● Blue   = Put roll
  ● Orange = Call roll
  ◆ Green  = Resync event
  Size     = Roll magnitude
```

### Roll Details Tooltip (on hover)
```
┌─ Roll Details ──────────────────┐
│ Day 1, 14:00                    │
│ [CALL] Up and Out               │
│ ├─ Strike: $75.00 → $75.50      │
│ ├─ Expiration: Day 1 → Day 3    │
│ ├─ Premium: $1.50 (new)         │
│ ├─ Intrinsic converted: $0.25   │
│ └─ Reason: Deep ITM (1.50 pts)  │
└─────────────────────────────────┘
```

### Roll History Panel (Post-Simulation)
```
┌─ Roll History Summary ──────────────┐
│                                     │
│ Position #1 (Short Straddle)        │
│ ├─ Total Rolls: 12                  │
│ ├─ Tighten: 3                       │
│ ├─ Up/Down & Out: 2                 │
│ ├─ Recenter: 6                      │
│ └─ Resync: 1                        │
│                                     │
│ Timeline:                           │
│ Day 0  ████████████████████ Open    │
│ Day 1  ██▓▓░░██████████████ Tighten │
│        ██▓▓░░██▒▒░░████████ Up&Out  │
│ Day 2  ░░░░░░██▒▒░░████████ Held    │
│ Day 3  ░░░░░░██▒▒░░██░░░░░░ Resync  │
│                                     │
│ █ Put    ▓ Call    ░ Gap    ▒ Diverged│
│                                     │
└─────────────────────────────────────┘
```

---

## 4. State Badges (Priority #3)

### Position Status Indicator
```
┌─ Position Status ─────────┐
│                           │
│ Short #1  [NORMAL]        │
│   Put: $75.00  Day 5      │
│   Call: $75.00  Day 5     │
│                           │
│ Long #1   [EXTENDED]      │
│   Put: $73.00  Day 45     │
│   Call: $77.00  Day 45    │
│                           │
└───────────────────────────┘
```

### Badge Types
- `[NORMAL]` — Green — Both legs at same strike, same expiration
- `[INVERTED]` — Yellow — Put strike > Call strike (spread shown)
- `[DIVERGED]` — Orange — Legs have different expirations
- `[EXTENDED]` — Blue — One or both legs extended beyond normal DTE
- `[HELD]` — Purple — One leg held (not rolled) due to deep ITM

---

## 5. Leg Details Review (Post-Simulation Only)

### Display Location
Tab next to "Trade Log": `[Trade Log] [Leg Details] [Summary Stats]`

### Layout
```
┌─ Leg Details ─────────────────────────────────────────┐
│                                                        │
│ Position: Short #1                                    │
│ Status: [INVERTED]  Spread: 1.50                      │
│                                                        │
│ ┌─ PUT Leg ──────────────┬─ CALL Leg ────────────────┐│
│ │ Status: Normal         │ Status: Extended          ││
│ │ Strike: $76.50         │ Strike: $75.00            ││
│ │ Expiration: Day 6      │ Expiration: Day 7         ││
│ │ DTE: 2.4               │ DTE: 3.4                  ││
│ │                        │                           ││
│ │ Entry: $0.60 ($600)    │ Entry: $0.60 ($600)       ││
│ │ Current: $0.15 ($150)  │ Current: $1.50 ($1500)    ││
│ │                        │                           ││
│ │ P&L: +$450 (75%)       │ P&L: -$900 (-150%)        ││
│ │ Status: Deep OTM       │ Status: Deep ITM          ││
│ │                        │                           ││
│ │ Roll History:          │ Roll History:             ││
│ │ • Day 1: Tighten       │ • Day 1: Up & Out         ││
│ │   $75→$76.50           │   $75→$75.50 (+2 days)    ││
│ │   Reason: 92% profit   │   Reason: 1.50 intrinsic  ││
│ │ • Day 1: Recenter      │                           ││
│ │   $76.50→$76.75        │                           ││
│ └────────────────────────┴───────────────────────────┘│
│                                                        │
│ Combined P&L: -$450                                    │
│                                                        │
└────────────────────────────────────────────────────────┘
```

---

## 6. Implementation Priority

| Priority | Feature | Why First |
|----------|---------|-----------|
| 1 | Roll History Visualization | Most important for verifying correctness |
| 2 | Pair-based Configuration | Foundation for all other features |
| 3 | Trade Log with Filters | Essential for debugging and analysis |
| 4 | CSV Export | Allows Excel analysis |
| 5 | State Badges | Quick visual status |
| 6 | Leg Details Panel | Review after simulation |
| 7 | P&L Breakdown | Nice to have |

---

## 7. Open Questions

1. **Maximum pairs**: How many short/long pairs should we support? (Suggested: max 3 short + 3 long = 6 pairs)

2. **Pair naming**: Auto-number (Short #1, Short #2) or custom labels?

3. **Trigger inheritance**: Should new pairs copy triggers from previous pair, or start fresh?

4. **CSV format**: Include Greeks in export? (Delta, Gamma, Theta at time of roll)

5. **Real-time vs post-sim**: Should roll history update live during simulation, or only at end?

---

## Summary

**Key Changes from Current UI:**
- Replace single strategy selector with "Add Pair" buttons
- Each pair configurable independently (DTE, offset, triggers)
- Left panel contains all configuration, collapsible
- Trade log gets filter tabs and CSV export
- New "Roll History" visualization on chart
- Post-simulation leg details in separate tab
- State badges show Normal/Inverted/Diverged at a glance

**User Workflow:**
1. Add Short Pair → Configure (straddle/strangle, offset, DTE, triggers)
2. Add Long Pair (optional) → Configure
3. Set global parameters (days, price, vol)
4. Run Simulation
5. Review roll history on chart
6. Filter trade log as needed
7. Export to CSV for Excel analysis
8. Check leg details tab for deep dive

---

*Document Version: 1.0*
*Requirements Source: Stefan (2026-02-14)*
