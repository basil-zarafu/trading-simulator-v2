# Strategy Engine Design

**Status**: Design Phase  
**Owner**: Basil & Stefan  
**Priority**: CRITICAL — Foundation for all future work

## Vision

The strategy engine is the **core decision-making system**. It must be:
- **Correct**: Every decision is auditable and verifiable
- **Flexible**: Support any strategy we can imagine
- **Fast**: Make decisions in microseconds (Monte Carlo scale)
- **Extensible**: Easy to add new strategy types

## Research: How Others Do It

### 1. Backtrader (Python)

**Approach**: Event-driven, callback-based
```python
def next(self):
    if self.position:
        if self.data.close > self.sma:
            self.close()
    else:
        if self.data.close < self.sma:
            self.buy()
```

**Pros**: Simple, intuitive  
**Cons**: Mutable state, hard to debug, no replay

### 2. Zipline (Quantopian)

**Approach**: Pipeline-based, factor modeling
```python
def initialize(context):
    context.assets = [symbol('AAPL')]

def handle_data(context, data):
    order_target_percent(symbol('AAPL'), 0.5)
```

**Pros**: Clean separation of concerns  
**Cons**: Heavy framework, limited flexibility

### 3. Lean (QuantConnect)

**Approach**: Event-driven with data structures
```csharp
public override void OnData(Slice data) {
    if (!Portfolio.Invested) {
        SetHoldings("SPY", 1.0);
    }
}
```

**Pros**: Professional, fast, well-tested  
**Cons**: Complex, steep learning curve

### 4. Our Approach (V2)

**Event Sourcing + Pure Functions**

```rust
// Strategy is a pure function: (State, Event) -> (State, Actions)
fn on_event(state: &PositionState, event: &MarketEvent) -> Vec<Action> {
    match event {
        MarketEvent::PriceTick { price, time } => {
            if should_roll(state, price, time) {
                vec![Action::Roll { leg_id: state.leg_id }]
            } else {
                vec![]
            }
        }
        _ => vec![]
    }
}
```

**Why this is better:**
- **Replay**: Can re-run any simulation with exact same decisions
- **Test**: Pure functions are easy to unit test
- **Debug**: Every decision logged with full context
- **Parallel**: No shared mutable state = true parallelism

---

## Strategy Engine Requirements

### Functional Requirements

#### R1: Multi-Leg Strategies

**Must support:**
- Any number of legs (1, 2, 4, 10+)
- Each leg independent OR synchronized
- Different roll triggers per leg
- Mixed directions (short puts + long calls, etc.)

**Example strategies:**
```rust
// 1DTE Short Straddle (2 legs)
Strategy::ShortStraddle { dte: 1, roll_time: time!(14:00) }

// Iron Condor (4 legs)
Strategy::IronCondor { 
    put_spread: 2.0, 
    call_spread: 2.0,
    roll_mode: RollMode::Individual 
}

// Custom multi-leg
Strategy::Custom {
    legs: vec![
        Leg::short_put().otm(2.0).dte(1),
        Leg::short_call().otm(2.0).dte(1),
        Leg::long_put().otm(5.0).dte(30), // Protection
    ]
}
```

#### R2: Flexible Roll Triggers

**Each leg must support any combination:**

| Trigger | Description | Example |
|---------|-------------|---------|
| DTE Threshold | Roll when DTE <= X | Roll at 0 DTE (expiration) |
| Time-Based | Roll at specific time daily | Roll at 14:00 ET |
| Profit Target | Roll when P&L >= X% | Roll at 50% profit |
| Price Move | Roll when underlying moves X pts | Recenter at 3pt move |
| Delta Target | Roll when delta exceeds threshold | Roll when delta > 0.30 |
| Manual | User-initiated roll | Override for news |

**Combinations:**
- "Roll at 14:00 OR if profit target hit"
- "Roll at 28 DTE if profit target not hit"
- "Roll when price moves 3pts, but max once per day"

#### R3: Strike Selection

**Entry strike selection:**
- ATM (at-the-money)
- OTM by X points
- OTM by X delta
- Percentage of underlying (e.g., 98% for put)
- Fixed strike (for calendar spreads)

**Roll strike selection:**
- Same strike (calendar roll)
- New ATM
- Maintain offset (e.g., always 2 OTM)
- Adjust offset based on condition

#### R4: Position Management

**Position-level features:**
- Profit target (close entire position at X% profit)
- Stop loss (close at X% loss)
- Max delta exposure (add protection if delta > threshold)
- Time-based exit (close at 16:00 regardless)

**Cooldowns:**
- Per-leg daily cooldown (don't roll same leg twice in one day)
- Position-level cooldown
- Minimum time between rolls

#### R5: Risk Management

**Built-in guards:**
- Max position size
- Max margin usage
- Concentration limits (not too much in one expiration)
- Circuit breakers (pause if volatility spikes)

### Non-Functional Requirements

#### R6: Performance

| Metric | Target | Reason |
|--------|--------|--------|
| Decision latency | < 1 µs | Monte Carlo (1M events) |
| Simulations/second | > 10,000 | Parameter sweeps |
| Memory per sim | < 10 MB | Run 1000 in parallel |
| Event throughput | > 1M events/sec | Large price files |

#### R7: Correctness

**Must be provably correct:**
- No race conditions
- No off-by-one errors in DTE calculation
- No double-counting commissions
- No missed rolls
- No rolls on non-trading days

**Validation:**
- Property-based testing (invariants always hold)
- Golden tests (known inputs → known outputs)
- Differential testing (V1 vs V2)

#### R8: Observability

**Every decision logged:**
```rust
struct DecisionLog {
    timestamp: DateTime,
    leg_id: u8,
    trigger: RollTrigger,
    context: DecisionContext,  // Price, Greeks, P&L, etc.
    action: Action,
    reasoning: String,  // Human-readable explanation
}
```

**Query capabilities:**
- "Show all rolls on Jan 15"
- "Why didn't we roll on Jan 16?"
- "What was the P&L impact of the 14:00 roll vs 16:00 roll?"

#### R9: Extensibility

**Easy to add:**
- New strategy types (plugins?)
- New roll triggers
- New products (/ES, stocks)
- New pricing models

**Plugin architecture:**
```rust
trait StrategyPlugin {
    fn on_event(&self, state: &State, event: &Event) -> Vec<Action>;
    fn validate(&self) -> Result<(), ValidationError>;
}
```

---

## Architecture

### Component Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    Strategy Engine                       │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─────────────┐    ┌─────────────┐    ┌────────────┐  │
│  │   Strategy  │───▶│  Decision   │───▶│  Action    │  │
│  │   Config    │    │   Engine    │    │  Executor  │  │
│  └─────────────┘    └─────────────┘    └────────────┘  │
│         │                   │                  │        │
│         ▼                   ▼                  ▼        │
│  ┌─────────────┐    ┌─────────────┐    ┌────────────┐  │
│  │    Leg      │    │   Trigger   │    │   Event    │  │
│  │   State     │◀───│   Engine    │◀───│   Store    │  │
│  └─────────────┘    └─────────────┘    └────────────┘  │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Key Components

#### 1. Strategy Config

Immutable configuration defining the strategy.

```rust
struct StrategyConfig {
    name: String,
    legs: Vec<LegConfig>,
    roll_logic: RollLogic,
    risk_limits: RiskLimits,
}

struct LegConfig {
    leg_id: u8,
    instrument: InstrumentConfig,
    direction: Direction,
    
    // Entry
    entry_dte: u8,
    strike_selection: StrikeSelection,
    
    // Roll triggers (any combination)
    triggers: Vec<RollTrigger>,
    
    // Roll execution
    roll_strike_selection: StrikeSelection,
    roll_to_dte: u8,
}

enum RollTrigger {
    DteThreshold { dte: f64 },
    TimeOfDay { time: Time },
    ProfitTarget { pct: f64 },
    PriceMove { points: f64 },
    DeltaThreshold { delta: f64 },
    Manual,
}
```

#### 2. Decision Engine

Pure function that decides what to do.

```rust
struct DecisionEngine;

impl DecisionEngine {
    fn evaluate(
        &self,
        state: &PositionState,
        event: &MarketEvent,
        config: &LegConfig,
    ) -> Vec<Action> {
        // Check all triggers
        let triggered: Vec<_> = config.triggers
            .iter()
            .filter(|t| self.check_trigger(t, state, event))
            .collect();
        
        if triggered.is_empty() {
            return vec![];
        }
        
        // Determine action
        vec![Action::Roll {
            leg_id: state.leg_id,
            reasons: triggered,
        }]
    }
    
    fn check_trigger(
        &self,
        trigger: &RollTrigger,
        state: &PositionState,
        event: &MarketEvent,
    ) -> bool {
        match trigger {
            RollTrigger::DteThreshold { dte } => {
                state.dte <= *dte
            }
            RollTrigger::TimeOfDay { time } => {
                event.time() >= *time && !state.rolled_today
            }
            RollTrigger::ProfitTarget { pct } => {
                state.pnl_pct >= *pct
            }
            // ... etc
        }
    }
}
```

#### 3. Action Executor

Executes actions and records events.

```rust
struct ActionExecutor<'a> {
    event_store: &'a mut EventStore,
    pricing_engine: &'a PricingEngine,
}

impl<'a> ActionExecutor<'a> {
    fn execute(&mut self, state: &mut PositionState, action: &Action) -> Result<()> {
        match action {
            Action::Open { contract } => {
                let price = self.pricing_engine.price(contract)?;
                let event = Event::PositionOpened {
                    contract: contract.clone(),
                    price,
                    // ...
                };
                self.event_store.record(event)?;
                state.apply(event)?;
            }
            Action::Roll { leg_id, reasons } => {
                // Close old
                let old_contract = &state.contract;
                let exit_price = self.pricing_engine.price(old_contract)?;
                
                // Open new
                let new_contract = self.determine_new_contract(state)?;
                let entry_price = self.pricing_engine.price(&new_contract)?;
                
                let event = Event::PositionRolled {
                    old_contract: old_contract.clone(),
                    new_contract,
                    exit_price,
                    entry_price,
                    reasons: reasons.clone(),
                    // ...
                };
                self.event_store.record(event)?;
                state.apply(event)?;
            }
            // ... etc
        }
        Ok(())
    }
}
```

#### 4. Event Store

Immutable log of all events.

```rust
trait EventStore {
    fn record(&mut self, event: Event) -> Result<EventId>;
    fn get(&self, id: EventId) -> Option<&Event>;
    fn query(&self, filter: EventFilter) -> Vec<&Event>;
    fn all(&self) -> &[Event];
}

// In-memory implementation
struct InMemoryEventStore {
    events: Vec<Event>,
}

// Persistent implementation (SQLite, Parquet)
struct PersistentEventStore {
    db: Connection,
}
```

---

## Monte Carlo Integration

The strategy engine must work seamlessly with Monte Carlo:

```rust
struct MonteCarloEngine {
    strategy_engine: StrategyEngine,
    price_generator: PriceGenerator,
}

impl MonteCarloEngine {
    fn run_study(&self, config: &StudyConfig) -> StudyResults {
        let results: Vec<_> = config.seeds
            .par_iter()  // Parallel execution
            .map(|seed| self.run_single(*seed, config))
            .collect();
        
        StudyResults::aggregate(results)
    }
    
    fn run_single(&self, seed: u64, config: &StudyConfig) -> SimulationResult {
        let price_path = self.price_generator.generate(seed, &config.model);
        let events = self.strategy_engine.run(&config.strategy, &price_path);
        SimulationResult::from_events(events)
    }
}
```

**Key insight:** Strategy engine doesn't know it's in a Monte Carlo. It just processes events.

---

## Interface Design

### For Stefan (User Interface)

```rust
// Define a strategy
let strategy = Strategy::builder()
    .short_strangle()
    .put_otm(2.0)
    .call_otm(2.0)
    .roll_at_dte(0)
    .roll_at_time(time!(14:00))
    .profit_target(0.50)
    .build()?;

// Run single simulation
let result = engine.run(&strategy, &price_data)?;
println!("P&L: ${}", result.net_pnl);

// Run Monte Carlo
let study = MonteCarlo::new(&strategy)
    .seeds(0..1000)
    .model(Schwartz2F::with_params(kappa, sigma, ...))
    .run()?;

println!("Win rate: {:.1}%", study.win_rate() * 100.0);
```

### For Developers (Extension Interface)

```rust
// Custom strategy
trait Strategy {
    fn on_event(&self, state: &PositionState, event: &Event) -> Vec<Action>;
}

struct MyCustomStrategy;

impl Strategy for MyCustomStrategy {
    fn on_event(&self, state: &PositionState, event: &Event) -> Vec<Action> {
        // Custom logic
        vec![]
    }
}

// Custom trigger
struct MyCustomTrigger { threshold: f64 }

impl RollTrigger for MyCustomTrigger {
    fn check(&self, state: &PositionState, event: &Event) -> bool {
        state.some_metric > self.threshold
    }
}
```

---

## Open Questions

1. **Strategy definition format:**
   - Rust code (compile-time safety)
   - YAML/JSON config (runtime flexibility)
   - Domain-specific language (DSL)

2. **Plugin system:**
   - Dynamic loading (.dll/.so files)
   - WebAssembly (sandboxed, portable)
   - Just recompile (simplest, safest)

3. **State persistence:**
   - In-memory only (fast, but lost on crash)
   - SQLite (durable, queryable)
   - Parquet (compressed, analytics-friendly)

4. **Rollback capability:**
   - Can we undo a decision? (probably not — events are immutable)
   - Or do we restart simulation from beginning?

---

**Next Steps:**
1. Review this document thoroughly
2. Discuss open questions
3. Create ADR for final design
4. Begin implementation of core engine

**Your input needed on:**
- Which roll triggers are must-have for Phase 1?
- Do we need dynamic strategy loading, or recompile is OK?
- What's the simplest way for you to define strategies?
