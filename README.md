# Trading Simulator V2

## Project Vision

A professional-grade oil futures (/CL) options backtesting and research platform designed for:
- Systematic strategy validation through Monte Carlo simulation
- Clear, auditable, and trustworthy results
- Extensible architecture for future products and strategies
- Reinforcement Learning integration for position management optimization

## Design Principles

1. **Correctness First**: Every calculation must be verifiable and auditable
2. **Transparency**: Full event logging and replay capability
3. **Testability**: Every component unit-testable with known inputs/outputs
4. **Extensibility**: Easy to add new products, strategies, and features
5. **Performance**: Efficient Monte Carlo sweeps (10,000+ sims in minutes)

## Project Structure

```
trading-simulator-v2/
├── docs/                          # Documentation
│   ├── architecture/              # Design docs
│   ├── decisions/                 # ADRs (Architecture Decision Records)
│   └── api/                       # API documentation
├── src/                           # Source code
│   ├── core/                      # Price models, pricing, calendar
│   ├── simulation/                # Strategy engine, execution
│   ├── analysis/                  # Monte Carlo, statistics, reporting
│   └── validation/                # Testing, invariants, golden tests
├── tests/                         # Test suite
│   ├── unit/                      # Unit tests
│   ├── integration/               # Integration tests
│   └── golden/                    # Reference simulations
├── examples/                      # Example strategies
├── notebooks/                     # Research notebooks
└── scripts/                       # Utility scripts
```

## Development Phases

### Phase 1: Planning & Design (Current)
- [ ] Language selection with justification
- [ ] Core architecture design
- [ ] Domain modeling (products, strategies, events)
- [ ] Validation strategy
- [ ] Testing framework design

### Phase 2: Core Foundation
- [ ] Price generation (Schwartz 2F, GBM)
- [ ] Options pricing (Black-76)
- [ ] Trading calendar
- [ ] Event system foundation

### Phase 3: Simulation Engine
- [ ] Strategy engine
- [ ] Execution engine
- [ ] Accounting/PNL tracking
- [ ] Roll logic

### Phase 4: Analysis & Validation
- [ ] Monte Carlo framework
- [ ] Statistics calculation
- [ ] Reporting/visualization
- [ ] Golden test suite

### Phase 5: Application Layer
- [ ] CLI interface
- [ ] Desktop app (future)
- [ ] Data export/import

## Documentation

All decisions and designs are documented in `docs/`:
- `architecture/` — System design and component interactions
- `decisions/` — Architecture Decision Records (ADRs)
- `api/` — API documentation and examples

## Getting Started

See individual documents in `docs/` for detailed planning.

---

**Status**: Phase 1 — Planning & Design  
**Last Updated**: 2026-02-11
