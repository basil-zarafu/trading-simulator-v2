# Domain Model

**Status**: Design Phase  
**Purpose**: Define core entities and their relationships

## Core Concepts

### 1. Product

A tradable underlying asset.

```rust
struct Product {
    symbol: String,           // "/CL"
    name: String,             // "Crude Oil Futures"
    tick_size: f64,           // 0.01
    point_value: f64,         // $1000 per point
    trading_hours: TradingHours,
    expiration_rules: ExpirationRules,
    strike_increment: f64,    // 0.25 for /CL
}
```

**Products to support (future):**
- /CL (Crude Oil) — Phase 1
- /ES (E-mini S&P) — Phase 2
- Individual stocks — Phase 3

---

### 2. Option Contract

A single option leg.

```rust
struct OptionContract {
    underlying: Product,
    option_type: OptionType,  // Call or Put
    strike: f64,
    expiration: DateTime,
    direction: Direction,     // Long or Short
}

enum OptionType { Call, Put }
enum Direction { Long, Short }
```

---

### 3. Strategy

A multi-leg position configuration.

```rust
struct Strategy {
    name: String,
    legs: Vec<LegConfig>,
    roll_logic: RollLogic,
}

struct LegConfig {
    leg_id: u8,
    option_type: OptionType,
    direction: Direction,
    entry_dte: u8,            // Days to expiration at entry
    strike_selection: StrikeSelection,
    
    // Roll triggers
    roll_trigger_dte: u8,     // Roll when DTE <= this
    roll_time: Time,          // Time of day to roll (14:00)
    profit_target_pct: Option<f64>,  // Optional: roll at X% profit
}

enum StrikeSelection {
    ATM,                       // At-the-money
    OTM { offset: f64 },       // OTM by X points
    Delta { target: f64 },     // Target delta (future)
}

enum RollLogic {
    Independent,              // Each leg rolls on its own triggers
    Synchronized,             // All legs roll together
}
```

**Strategies to support:**
- Short Straddle (ATM) — Phase 1
- Short Strangle (OTM) — Phase 1
- Iron Condor — Phase 2
- Custom multi-leg — Phase 2

---

### 4. Price Path

Time series of underlying prices.

```rust
struct PricePath {
    product: Product,
    timestamps: Vec<DateTime>,
    prices: Vec<PriceBar>,
    source: PriceSource,
}

struct PriceBar {
    timestamp: DateTime,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: Option<u64>,
}

enum PriceSource {
    Generated(PriceModel),    // Synthetic (Schwartz, GBM)
    Historical,               // Real data (QuantConnect)
    Hybrid,                   // Historical + synthetic vol
}

enum PriceModel {
    GBM(GBMParams),
    Schwartz1F(Schwartz1FParams),
    Schwartz2F(Schwartz2FParams),
}
```

---

### 5. Event (Core of Event Sourcing)

Every state change is an immutable event.

```rust
enum Event {
    // Market data
    PriceTick { timestamp: DateTime, price: f64 },
    
    // Position lifecycle
    PositionOpened {
        timestamp: DateTime,
        leg_id: u8,
        contract: OptionContract,
        entry_price: f64,
        credit_debit: f64,     // Positive = credit (short), negative = debit (long)
    },
    
    PositionRolled {
        timestamp: DateTime,
        leg_id: u8,
        old_contract: OptionContract,
        new_contract: OptionContract,
        exit_price: f64,
        entry_price: f64,
        realized_pnl: f64,
        commission: f64,
        reason: RollReason,
    },
    
    PositionClosed {
        timestamp: DateTime,
        leg_id: u8,
        exit_price: f64,
        realized_pnl: f64,
        commission: f64,
    },
    
    // Mark to market
    MarkToMarket {
        timestamp: DateTime,
        leg_id: u8,
        underlying: f64,
        option_price: f64,
        unrealized_pnl: f64,
    },
}

enum RollReason {
    DteThreshold,      // DTE reached trigger level
    TimeBased,         // Scheduled roll time
    ProfitTarget,      // Profit target hit
    PriceRecenter,     // Price moved, recentering
    Manual,            // User-initiated
}
```

**Key insight:** With events, we can:
- Replay any simulation exactly
- Debug by inspecting event sequence
- Verify accounting by summing events
- Export to multiple formats

---

### 6. Position State

Current state of a position leg (derived from events).

```rust
struct PositionState {
    leg_id: u8,
    contract: OptionContract,
    entry_timestamp: DateTime,
    entry_price: f64,
    entry_credit_debit: f64,
    
    // Current state
    current_price: f64,
    unrealized_pnl: f64,
    
    // Roll tracking
    roll_count: u32,
    last_roll_timestamp: Option<DateTime>,
    rolled_today: bool,
    
    // Accumulated
    total_realized_pnl: f64,
    total_commissions: f64,
}
```

---

### 7. Simulation Result

Output of a single simulation run.

```rust
struct SimulationResult {
    simulation_id: String,
    strategy: Strategy,
    price_path: PricePath,
    events: Vec<Event>,
    
    // Final state
    final_pnl: f64,
    final_commissions: f64,
    final_net_pnl: f64,
    
    // Statistics
    total_trades: u32,
    winning_trades: u32,
    losing_trades: u32,
    max_drawdown: f64,
    sharpe_ratio: Option<f64>,
    
    // Daily series
    equity_curve: Vec<(DateTime, f64)>,
}
```

---

### 8. Monte Carlo Study

Collection of simulation results.

```rust
struct MonteCarloStudy {
    name: String,
    strategy: Strategy,
    parameters: StudyParameters,
    results: Vec<SimulationResult>,
}

struct StudyParameters {
    num_simulations: usize,
    seeds: Vec<u64>,
    price_model: PriceModel,
    model_parameter_ranges: HashMap<String, ParameterRange>,
}

struct ParameterRange {
    min: f64,
    max: f64,
    distribution: Distribution,
}
```

---

## Entity Relationships

```
Product --has-many--> OptionContract
Strategy --has-many--> LegConfig
PricePath --generates--> Events
Events --produce--> PositionState
PositionState --summarized-in--> SimulationResult
SimulationResult --aggregated-in--> MonteCarloStudy
```

---

## Key Design Decisions

### 1. Events are the Source of Truth

Don't store mutable state. Store events, derive state.

**Why:**
- Replay capability
- Audit trail
- Easy debugging
- Time-travel queries

### 2. Separated Concerns

| Component | Responsibility |
|-----------|---------------|
| PriceGenerator | Produce price paths |
| StrategyEngine | Decide when to trade |
| ExecutionEngine | Record trades (events) |
| AccountingEngine | Calculate P&L from events |
| AnalysisEngine | Statistics, reporting |

### 3. Immutable Data

Once created, data structures don't change. New events create new state.

**Why:**
- No side effects
- Thread-safe
- Easy to reason about
- Cheap to clone/share

---

## Open Questions

1. **Persistence:** Store events in memory only, or write to disk (SQLite, Parquet)?
2. **Compression:** Monte Carlo studies could be huge (millions of events). Compress?
3. **Streaming:** Process events as they happen, or batch at end?
4. **Snapshots:** Keep periodic state snapshots for fast queries, or always replay?

---

**Next Steps:**
1. Review domain model
2. Approve or revise
3. Create ADR documenting final design
4. Begin implementation
