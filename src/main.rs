//! Trading Simulator V2 - Phase 2: GBM Price Generator
//!
//! Demonstrates:
//! - Synthetic calendar
//! - Event sourcing
//! - GBM price generation
//! - 1DTE straddle strategy with realistic prices
//!
//! Timing convention:
//! - First position: opened at 15:00 day 0, rolled at 14:00 day 1 (23 hours)
//! - Subsequent: opened at 14:00, rolled at 14:00 next day (24 hours)
//! - Each position expires at 14:30 on its expiration day

mod calendar;
mod events;
mod prices;

use calendar::{Calendar, Day, TimeOfDay};
use events::{Event, EventStore, OptionContract, OptionType, PositionId, Side, LegId};
use prices::GBM;

/// Time constants (in minutes from midnight)
const ENTRY_TIME: TimeOfDay = 15 * 60;       // 15:00
const ROLL_TIME: TimeOfDay = 14 * 60;        // 14:00
const EXPIRY_TIME: TimeOfDay = 14 * 60 + 30; // 14:30

/// Simulation parameters
#[derive(Debug)]
struct SimulationConfig {
    /// Starting day (0 = Monday, Jan 1, Year 0)
    start_day: Day,
    /// Number of days to simulate
    num_days: usize,
    /// Initial price (e.g., 75.0 for /CL at $75)
    initial_price: f64,
    /// Annual drift (μ), e.g., 0.0 for no drift
    drift: f64,
    /// Annual volatility (σ), e.g., 0.30 for 30%
    volatility: f64,
    /// Random seed for reproducibility
    seed: u64,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            start_day: 0,
            num_days: 30,
            initial_price: 75.0,
            drift: 0.0,      // No drift
            volatility: 0.30, // 30% annual vol
            seed: 42,
        }
    }
}

/// Simple 1DTE straddle strategy configuration
#[derive(Debug)]
struct StraddleConfig {
    /// How many points OTM for each leg (0.0 = ATM)
    strike_offset: f64,
}

impl Default for StraddleConfig {
    fn default() -> Self {
        Self {
            strike_offset: 0.0, // ATM
        }
    }
}

/// Current state of a running straddle position
#[derive(Debug)]
struct StraddleState {
    position_id: PositionId,
    put_leg: LegId,
    call_leg: LegId,
    /// Day this position was opened
    entry_day: Day,
    /// Day this position expires (14:30)
    expiration_day: Day,
    /// Entry price of underlying
    entry_price: f64,
}

fn main() {
    println!("Trading Simulator V2 - Phase 2: GBM Price Generator\n");

    let calendar = Calendar::new();
    let mut event_store = EventStore::new();
    let sim_config = SimulationConfig::default();
    let strat_config = StraddleConfig::default();

    // Generate price path using GBM
    let mut gbm = GBM::new(
        sim_config.initial_price,
        sim_config.drift,
        sim_config.volatility,
        sim_config.seed,
    );
    let price_path = gbm.generate_path(sim_config.num_days);

    println!("Simulation parameters:");
    println!("  Start day: {} (Monday)", sim_config.start_day);
    println!("  Initial price: ${:.2}", sim_config.initial_price);
    println!("  Drift (μ): {:.2}%", sim_config.drift * 100.0);
    println!("  Volatility (σ): {:.0}%", sim_config.volatility * 100.0);
    println!("  Seed: {}", sim_config.seed);
    println!("  Entry time: 15:00");
    println!("  Roll time: 14:00 (next day)");
    println!();

    // Track active position
    let mut active_position: Option<StraddleState> = None;

    // Run simulation day by day
    let mut price_iter = price_path.iter();
    
    for day in sim_config.start_day..sim_config.start_day + sim_config.num_days as u32 {
        if !calendar.is_trading_day(day) {
            continue;
        }

        // Get price for this day
        let current_price = price_iter
            .next()
            .map(|(_, price)| *price)
            .unwrap_or_else(|| {
                // Fallback: use last known price
                price_path.last().map(|(_, p)| *p).unwrap_or(sim_config.initial_price)
            });

        let date_str = format_day(day);

        // Check for roll trigger at 14:00
        if let Some(pos) = active_position.take() {
            // We have an active position - check if today is the roll day
            if day == pos.expiration_day {
                // Roll at 14:00: close old position, open new one
                print!("Day {} ({}): Price ${:.2} | ", day, date_str, current_price);

                // Close current position at 14:00
                let close_event = Event::PositionClosed {
                    position_id: pos.position_id,
                    timestamp: (day, ROLL_TIME),
                    close_premiums: vec![
                        (pos.put_leg, 0.10), // Simulated decayed value
                        (pos.call_leg, 0.10),
                    ],
                    reason: events::CloseReason::Expiration,
                };
                event_store.append(close_event);
                println!("CLOSED position {} at 14:00", pos.position_id.0);

                // Open new position at 14:00 (expires next trading day)
                let new_position = open_position(
                    &calendar,
                    &mut event_store,
                    &strat_config,
                    day,
                    ROLL_TIME, // Open at roll time (14:00)
                    current_price,
                );
                println!(
                    "  -> OPENED position {} at 14:00 -> Exp {}",
                    new_position.position_id.0,
                    new_position.expiration_day
                );

                active_position = Some(new_position);
                continue; // Skip the entry check below
            }
        }

        // Check if we need to open a new position at 15:00
        if active_position.is_none() {
            print!("Day {} ({}): Price ${:.2} | ", day, date_str, current_price);

            let position = open_position(
                &calendar,
                &mut event_store,
                &strat_config,
                day,
                ENTRY_TIME, // Open at 15:00
                current_price,
            );
            println!(
                "OPENED position {} at 15:00 -> Exp {}",
                position.position_id.0,
                position.expiration_day
            );

            active_position = Some(position);
        } else {
            // Just holding
            let pos = active_position.as_ref().unwrap();
            let remaining_dte = calendar.calculate_dte(day, pos.expiration_day);
            println!(
                "Day {} ({}): Price ${:.2} | Holding position {} (DTE: {})",
                day, date_str, current_price, pos.position_id.0, remaining_dte
            );
        }
    }

    // Summary
    println!("\n=== Simulation Summary ===");
    println!("Total events recorded: {}", event_store.all_events().len());
    println!(
        "Final price: ${:.2}",
        price_path.last().map(|(_, p)| *p).unwrap_or(sim_config.initial_price)
    );

    // Replay events
    println!("\n=== Event Log ===");
    for event in event_store.all_events() {
        match event {
            Event::PositionOpened {
                position_id,
                timestamp,
                legs,
            } => {
                println!(
                    "\nPosition {} opened on day {} at {}:{:02}",
                    position_id.0,
                    timestamp.0,
                    timestamp.1 / 60,
                    timestamp.1 % 60
                );
                for (leg_id, contract, premium) in legs {
                    println!(
                        "  Leg {}: {:?} {:?} strike=${:.2} premium=${:.2}",
                        leg_id.0, contract.side, contract.option_type, contract.strike, premium
                    );
                }
            }
            Event::PositionClosed {
                position_id,
                timestamp,
                reason,
                ..
            } => {
                println!(
                    "Position {} closed on day {} at {}:{:02} (reason: {:?})",
                    position_id.0,
                    timestamp.0,
                    timestamp.1 / 60,
                    timestamp.1 % 60,
                    reason
                );
            }
            _ => {}
        }
    }
}

/// Open a new 1DTE straddle position
fn open_position(
    calendar: &Calendar,
    event_store: &mut EventStore,
    config: &StraddleConfig,
    entry_day: Day,
    entry_time: TimeOfDay,
    current_price: f64,
) -> StraddleState {
    // Expires on the next trading day at 14:30
    let expiration_day = calendar.next_trading_day(entry_day);

    let position_id = event_store.next_position_id();
    let put_leg_id = event_store.next_leg_id();
    let call_leg_id = event_store.next_leg_id();

    let put_strike = current_price - config.strike_offset;
    let call_strike = current_price + config.strike_offset;

    let put_contract = OptionContract {
        underlying_price: current_price,
        strike: put_strike,
        option_type: OptionType::Put,
        side: Side::Short,
        expiration_day,
    };

    let call_contract = OptionContract {
        underlying_price: current_price,
        strike: call_strike,
        option_type: OptionType::Call,
        side: Side::Short,
        expiration_day,
    };

    // Simulate premiums (in real code, from Black-Scholes)
    let put_premium = 0.50;
    let call_premium = 0.50;

    let event = Event::PositionOpened {
        position_id,
        timestamp: (entry_day, entry_time),
        legs: vec![
            (put_leg_id, put_contract, put_premium),
            (call_leg_id, call_contract, call_premium),
        ],
    };

    event_store.append(event);

    StraddleState {
        position_id,
        put_leg: put_leg_id,
        call_leg: call_leg_id,
        entry_day,
        expiration_day,
        entry_price: current_price,
    }
}

/// Format a day number as a human-readable date string
fn format_day(day: Day) -> String {
    let weekday = match day % 7 {
        0 => "Mon",
        1 => "Tue",
        2 => "Wed",
        3 => "Thu",
        4 => "Fri",
        5 => "Sat",
        6 => "Sun",
        _ => "???",
    };
    let week = day / 7;
    format!("{} W{}", weekday, week)
}
