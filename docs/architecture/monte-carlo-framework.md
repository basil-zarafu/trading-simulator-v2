# Monte Carlo Framework Design

**Status**: Design Phase  
**Owner**: Basil & Stefan  
**Priority**: HIGH — Core requirement for all future projects

## Vision

A **universal testing framework** for trading strategies. Not just for /CL options, but for:
- Oil futures (/CL) — Phase 1
- Index futures (/ES, /NQ) — Phase 2
- Stocks — Phase 2
- Crypto — Phase 3
- Multi-asset portfolios — Phase 3

**Key principle:** Same interface, any asset, any strategy.

## Requirements

### R1: Synthetic Data Generation

**Price models to support:**

| Model | Use Case | Parameters |
|-------|----------|------------|
| GBM | Simple benchmarks | drift, volatility |
| Schwartz 1F | Mean-reverting commodities | kappa, sigma, alpha |
| Schwartz 2F | Realistic oil futures | kappa, sigma_spot, sigma_yield, rho |
| Heston | Stochastic volatility | params TBD |
| Custom | User-defined | Plugin interface |

**Requirements:**
- Deterministic (same seed → identical path)
- Fast (generate 10 years of minute data in < 1 second)
- Realistic (match historical volatilities and correlations)
- Flexible (switch models per study)

### R2: Real Data Integration (Bootstrapping)

**From QuantConnect or other sources:**

```rust
enum DataSource {
    Synthetic(PriceModel),
    Historical { 
        symbol: String,
        timeframe: TimeFrame, 
        source: HistoricalProvider 
    },
    Bootstrap {
        base_data: Vec<PriceBar>,
        method: BootstrapMethod,
        block_size: usize,
    },
}

enum BootstrapMethod {
    Stationary,      // Resample individual returns
    MovingBlock,     // Preserve local dependencies
    CircularBlock,   // Wrap-around for continuity
}
```

**Use cases:**
1. **Synthetic**: Test strategy against known price dynamics
2. **Historical**: See how strategy performed in actual past
3. **Bootstrap**: Generate thousands of "alternate histories" from real data

### R3: Parameter Sweeps

**Must support:**

```rust
// Single parameter sweep
let study = MonteCarlo::new(&strategy)
    .parameter("kappa", 0.5..=5.0, 20)  // 20 values from 0.5 to 5.0
    .seeds(0..100)
    .run()?;

// Multi-dimensional sweep
let study = MonteCarlo::new(&strategy)
    .parameter("kappa", vec![0.5, 1.0, 2.0])
    .parameter("sigma", vec![0.2, 0.3, 0.4])
    .parameter("profit_target", vec![None, Some(0.2), Some(0.5)])
    .seeds(0..50)  // 3 × 3 × 3 × 50 = 1,350 simulations
    .run()?;

// Grid search with constraints
let study = MonteCarlo::new(&strategy)
    .parameter_grid(grid! {
        kappa: [0.5, 1.0, 2.0],
        sigma: [0.2, 0.3, 0.4],
    })
    .constraint(|params| params.kappa * params.sigma < 1.0)  // Skip invalid combos
    .seeds(0..100)
    .run()?;
```

### R4: Seed Management

**Reproducibility is critical:**

```rust
// Predefined seed sets
const STAGE_1_SEEDS: [u64; 25] = generate_seeds(0, 1000_000_000, 25);
const STAGE_2_SEEDS: [u64; 100] = generate_seeds(0, 1000_000_000, 100);
const STAGE_3_SEEDS: [u64; 200] = generate_seeds(0, 1000_000_000, 200);

// Or load from file
let seeds: Vec<u64> = std::fs::read_to_string("seeds/testbed_150.txt")?
    .lines()
    .map(|s| s.parse().unwrap())
    .collect();

// Usage
let study = MonteCarlo::new(&strategy)
    .seeds(seeds)
    .run()?;
```

**Seed sets should be:**
- Version controlled
- Documented (where they came from, why these seeds)
- Reusable across studies
- Deterministic (don't change between runs)

### R5: Parallel Execution

**Must utilize all CPU cores:**

```rust
let study = MonteCarlo::new(&strategy)
    .seeds(0..10_000)
    .parallelism(Parallelism::Auto)  // Use all cores
    .run()?;

// Or manual
let study = MonteCarlo::new(&strategy)
    .seeds(0..10_000)
    .parallelism(Parallelism::Fixed(16))  // Use 16 threads
    .run()?;
```

**Implementation:**
- Rayon's parallel iterators (data parallelism)
- No shared mutable state (each simulation is independent)
- Progress reporting ("3,456 / 10,000 complete")
- Fault tolerance (one failed sim doesn't kill the whole study)

### R6: Results Storage & Querying

**Efficient storage for millions of simulations:**

```rust
// In-memory (small studies)
let results: Vec<SimulationResult> = study.results;

// Persistent (large studies)
let study = MonteCarlo::new(&strategy)
    .seeds(0..1_000_000)
    .storage(Storage::Parquet("study_results.parquet"))
    .run()?;

// Querying
let winners: Vec<&SimulationResult> = study
    .filter(|r| r.net_pnl > 0.0)
    .collect();

let by_seed: HashMap<u64, &SimulationResult> = study
    .group_by(|r| r.seed)
    .collect();
```

**Storage formats:**
| Format | Pros | Cons | Use Case |
|--------|------|------|----------|
| In-memory | Fastest | Lost on exit | Development, small studies |
| SQLite | Queryable, durable | Slower writes | Medium studies, complex queries |
| Parquet | Compressed, columnar | Immutable | Large studies, analytics |
| CSV | Human-readable | Bloated, slow | Export, sharing |

### R7: Statistical Analysis

**Built-in analytics:**

```rust
let stats = study.statistics();

// Basic
println!("Mean P&L: ${:.2}", stats.mean_pnl());
println!("Win rate: {:.1}%", stats.win_rate() * 100.0);
println!("Sharpe: {:.2}", stats.sharpe_ratio());

// Advanced
println!("VaR (95%): ${:.2}", stats.value_at_risk(0.95));
println!("Max drawdown: ${:.2}", stats.max_drawdown());
println!("Skewness: {:.2}", stats.skewness());

// Comparison
let baseline = load_study("baseline.parquet")?;
let comparison = study.compare(&baseline);
println!("Improvement: {:.1}%", comparison.pnl_improvement() * 100.0);
```

**Distributions:**
- P&L distribution (histogram)
- Equity curve percentiles (5th, 25th, 50th, 75th, 95th)
- Drawdown distribution
- Trade duration distribution

### R8: Visualization (Future)

**Generate plots:**

```rust
// Equity curves
study.plot_equity_curves()
    .percentiles([5, 25, 50, 75, 95])
    .save("equity_curves.png")?;

// P&L distribution
study.plot_pnl_distribution()
    .bins(50)
    .save("pnl_dist.png")?;

// Parameter sensitivity
study.plot_sensitivity("kappa")
    .save("kappa_sensitivity.png")?;
```

**Integration:**
- PNG/SVG export
- Interactive HTML (Plotly.js)
- Jupyter notebook widgets (via Python bindings)

---

## Architecture

### High-Level Flow

```
┌──────────────┐     ┌─────────────────┐     ┌──────────────┐
│   Study      │────▶│  Parameter      │────▶│  Simulation  │
│   Config     │     │  Generator      │     │  Queue       │
└──────────────┘     └─────────────────┘     └──────────────┘
                                                        │
                                                        ▼
┌──────────────┐     ┌─────────────────┐     ┌──────────────┐
│   Results    │◀────│  Result         │◀────│  Parallel    │
│   Aggregator │     │  Collector      │     │  Workers     │
└──────────────┘     └─────────────────┘     └──────────────┘
                                                        ▲
                                                        │
                                               ┌──────────────┐
                                               │  Price       │
                                               │  Generator   │
                                               └──────────────┘
                                                        ▲
                                                        │
                                               ┌──────────────┐
                                               │  Strategy    │
                                               │  Engine      │
                                               └──────────────┘
```

### Key Components

#### 1. Study Config

```rust
struct StudyConfig {
    name: String,
    strategy: StrategyConfig,
    data_source: DataSource,
    seeds: Vec<u64>,
    parameters: ParameterSpace,
    storage: StorageConfig,
    parallelism: Parallelism,
}

enum ParameterSpace {
    Single(Vec<(String, Vec<Value>)>),  // [("kappa", [0.5, 1.0, 2.0])]
    Grid(Vec<(String, Vec<Value>)>),    // Cross product of all values
    Constraint { 
        parameters: Vec<(String, Range<f64>)>,
        constraint: Box<dyn Fn(&Params) -> bool>,
    },
}
```

#### 2. Parameter Generator

```rust
struct ParameterGenerator;

impl ParameterGenerator {
    fn generate(&self, space: &ParameterSpace) -> impl Iterator<Item = Params> {
        match space {
            ParameterSpace::Single(params) => {
                // Generate all combinations
                cartesian_product(params)
            }
            ParameterSpace::Grid(params) => {
                // Same as Single
                cartesian_product(params)
            }
            ParameterSpace::Constraint { parameters, constraint } => {
                // Random sampling within constraints
                random_sample(parameters, constraint, 1000)
            }
        }
    }
}
```

#### 3. Simulation Worker

```rust
fn simulation_worker(
    config: &SimulationConfig,
    price_generator: &PriceGenerator,
    strategy_engine: &StrategyEngine,
) -> SimulationResult {
    // Generate price path
    let price_path = price_generator.generate(
        config.seed,
        &config.model_params
    );
    
    // Run strategy
    let events = strategy_engine.run(&config.strategy, &price_path);
    
    // Calculate results
    SimulationResult::from_events(events)
}
```

#### 4. Result Aggregator

```rust
struct ResultAggregator {
    results: Vec<SimulationResult>,
}

impl ResultAggregator {
    fn add(&mut self, result: SimulationResult) {
        self.results.push(result);
    }
    
    fn statistics(&self) -> StudyStatistics {
        let pnls: Vec<f64> = self.results.iter()
            .map(|r| r.net_pnl)
            .collect();
        
        StudyStatistics {
            count: self.results.len(),
            mean_pnl: mean(&pnls),
            std_pnl: std(&pnls),
            win_rate: count_positive(&pnls) as f64 / pnls.len() as f64,
            sharpe: mean(&pnls) / std(&pnls) * sqrt(252.0),
            max_drawdown: max_drawdown(&self.results),
            // ... etc
        }
    }
    
    fn filter<P>(&self, predicate: P) -> impl Iterator<Item = &SimulationResult>
    where P: Fn(&SimulationResult) -> bool {
        self.results.iter().filter(predicate)
    }
    
    fn group_by<K, F>(&self, key_fn: F) -> HashMap<K, Vec<&SimulationResult>>
    where K: Eq + Hash, F: Fn(&SimulationResult) -> K {
        // Group results by key
    }
}
```

---

## Interface Design

### For Stefan (High-Level)

```rust
fn main() -> Result<()> {
    // Define strategy
    let strategy = Strategy::short_strangle()
        .put_otm(2.0)
        .call_otm(2.0)
        .roll_at_dte(0)
        .build()?;
    
    // Define study
    let study = MonteCarlo::new(&strategy)
        .name("Strangle Offset Optimization")
        .parameter("put_offset", 0.5..=3.0, 10)
        .parameter("call_offset", 0.5..=3.0, 10)
        .seeds(load_seeds("seeds/stage_1_100.txt")?)
        .model(Schwartz2F::default())
        .run()?;
    
    // Results
    println!("Best parameters: {:?}", study.best_by_pnl()?);
    println!("Win rate by offset: {:?}", study.group_by("put_offset").win_rate()?);
    
    // Save
    study.save("strangle_study.parquet")?;
    
    Ok(())
}
```

### For Advanced Users

```rust
// Custom price model
struct MyCustomModel;

impl PriceModel for MyCustomModel {
    fn generate(&self, seed: u64, periods: usize) -> Vec<f64> {
        // Custom logic
    }
}

// Custom analysis
let study = MonteCarlo::new(&strategy)
    .seeds(0..1000)
    .custom_metric(|result| {
        // Calculate custom metric from result
        result.win_rate * result.avg_pnl
    })
    .run()?;
```

---

## Integration with Strategy Engine

**Key principle:** Monte Carlo doesn't know about strategies. Strategies don't know about Monte Carlo.

```rust
// Strategy Engine: Pure, event-driven
pub struct StrategyEngine;

impl StrategyEngine {
    pub fn run(&self, strategy: &Strategy, price_path: &PricePath) -> Vec<Event> {
        // Process events, return log
    }
}

// Monte Carlo: Orchestrates multiple runs
pub struct MonteCarlo<'a> {
    strategy_engine: &'a StrategyEngine,
}

impl<'a> MonteCarlo<'a> {
    pub fn run(&self, config: &StudyConfig) -> StudyResults {
        // For each parameter combo + seed:
        //   1. Generate price path
        //   2. Call strategy_engine.run()
        //   3. Collect results
        // Aggregate and return
    }
}
```

---

## Open Questions

1. **Real-time vs batch:**
   - Do we need to stream results as they complete?
   - Or wait for entire study to finish?

2. **Checkpointing:**
   - For long-running studies (millions of sims)
   - Save progress every 10,000 sims?
   - Resume after crash?

3. **Distributed execution:**
   - Multiple machines for huge studies?
   - Cloud support (AWS, GCP)?

4. **Memory management:**
   - Keep all results in memory?
   - Or stream to disk?
   - Configurable based on study size?

---

**Next Steps:**
1. Review this design
2. Decide on storage strategy (Parquet vs SQLite)
3. Define minimum viable Monte Carlo for Phase 1
4. Create ADR for final design

**For Phase 1, I recommend:**
- In-memory for studies < 100,000 sims
- Parquet export for persistence
- Single-machine parallelism (no distributed)
- Focus on correctness over features

Sound good?
