//! Trading Simulator V2 - Phase 3: Black-76 Pricing & P&L Tracking
//!
//! Demonstrates:
//! - Synthetic calendar
//! - Event sourcing
//! - GBM price generation
//! - Black-76 option pricing
//! - P&L tracking through position lifecycle
//! - Greeks at entry
//!
//! Timing convention:
//! - First position: opened at 15:00 day 0, rolled at 14:00 day 1 (23 hours)
//! - Subsequent: opened at 14:00, rolled at 14:00 next day (24 hours)
//! - Each position expires at 14:30 on its expiration day

mod calendar;
mod events;
mod prices;
mod pricing;

use calendar::{Calendar, Day, TimeOfDay};
use events::{Event, EventStore, OptionContract, OptionType, PositionId, Side, LegId, CloseReason};
use prices::GBM;
use pricing::{Black76, Greeks};

/// Time constants (in minutes from midnight)
const ENTRY_TIME: TimeOfDay = 15 * 60;       // 15:00
const ROLL_TIME: TimeOfDay = 14 * 60;        // 14:00
const EXPIRY_TIME: TimeOfDay = 14 * 60 + 30; // 14:30
const RISK_FREE_RATE: f64 = 0.05;            // 5% annual

/// Simulation parameters
#[derive(Debug)]
struct SimulationConfig {
    start_day: Day,
    num_days: usize,
    initial_price: f64,
    drift: f64,
    volatility: f64,
    seed: u64,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            start_day: 0,
            num_days: 30,
            initial_price: 75.0,
            drift: 0.0,
            volatility: 0.30,
            seed: 42,
        }
    }
}

/// Position tracking with P&L
#[derive(Debug)]
struct PositionTracking {
    position_id: PositionId,
    entry_day: Day,
    expiration_day: Day,
    entry_price: f64,
    put_strike: f64,
    call_strike: f64,
    put_entry_premium: f64,
    call_entry_premium: f64,
    put_greeks: Greeks,
    call_greeks: Greeks,
}

/// Track P&L summary
#[derive(Debug, Default)]
struct PnLSummary {
    total_premium_collected: f64,
    total_premium_paid: f64,
    position_count: u32,
}

fn main() {
    println!("Trading Simulator V2 - Phase 3: Black-76 Pricing & P&L\n");

    let calendar = Calendar::new();
    let mut event_store = EventStore::new();
    let sim_config = SimulationConfig::default();

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
    println!("  Risk-free rate: {:.1}%", RISK_FREE_RATE * 100.0);
    println!("  Seed: {}", sim_config.seed);
    println!("  Entry time: 15:00");
    println!("  Roll time: 14:00 (next day)");
    println!();

    // Track active position
    let mut active_position: Option<PositionTracking> = None;
    let mut pnl_summary = PnLSummary::default();

    // Run simulation day by day
    let mut price_iter = price_path.iter();
    
    for day in sim_config.start_day..sim_config.start_day + sim_config.num_days as u32 {
        if !calendar.is_trading_day(day) {
            continue;
        }

        let current_price = price_iter
            .next()
            .map(|(_, price)| *price)
            .unwrap_or_else(|| price_path.last().map(|(_, p)| *p).unwrap_or(sim_config.initial_price));

        let date_str = format_day(day);

        // Check for roll trigger at 14:00
        if let Some(pos) = active_position.take() {
            if day == pos.expiration_day {
                // Calculate actual close value (intrinsic at expiry)
                let put_close = calculate_close_value(current_price, pos.put_strike, false);
                let call_close = calculate_close_value(current_price, pos.call_strike, true);
                
                let position_pnl = (pos.put_entry_premium + pos.call_entry_premium) - (put_close + call_close);
                pnl_summary.total_premium_paid += put_close + call_close;
                
                print!("Day {} ({}): Price ${:.2} | ", day, date_str, current_price);
                println!("CLOSED position {} at 14:00 | P&L: ${:.2}", 
                         pos.position_id.0, position_pnl);

                let close_event = Event::PositionClosed {
                    position_id: pos.position_id,
                    timestamp: (day, ROLL_TIME),
                    close_premiums: vec![
                        (LegId(pos.position_id.0 * 2 - 1), put_close),
                        (LegId(pos.position_id.0 * 2), call_close),
                    ],
                    reason: CloseReason::Expiration,
                };
                event_store.append(close_event);

                // Open new position
                let new_pos = open_position_with_pricing(
                    &calendar,
                    &mut event_store,
                    &mut pnl_summary,
                    day,
                    ROLL_TIME,
                    current_price,
                    sim_config.volatility,
                );
                println!("  -> OPENED position {} at 14:00 | Put: ${:.2} Call: ${:.2} | Total: ${:.2}",
                         new_pos.position_id.0, 
                         new_pos.put_entry_premium, 
                         new_pos.call_entry_premium,
                         new_pos.put_entry_premium + new_pos.call_entry_premium);
                print_greeks(&new_pos);

                active_position = Some(new_pos);
                continue;
            }
        }

        // Open new position at 15:00
        if active_position.is_none() {
            let pos = open_position_with_pricing(
                &calendar,
                &mut event_store,
                &mut pnl_summary,
                day,
                ENTRY_TIME,
                current_price,
                sim_config.volatility,
            );
            
            print!("Day {} ({}): Price ${:.2} | ", day, date_str, current_price);
            println!("OPENED position {} at 15:00 | Put: ${:.2} Call: ${:.2} | Total: ${:.2}",
                     pos.position_id.0,
                     pos.put_entry_premium,
                     pos.call_entry_premium,
                     pos.put_entry_premium + pos.call_entry_premium);
            print_greeks(&pos);

            active_position = Some(pos);
        } else {
            let pos = active_position.as_ref().unwrap();
            let remaining_dte = calendar.calculate_dte(day, pos.expiration_day);
            
            // Calculate current position value (for tracking)
            let time_to_expiry = remaining_dte as f64 / 252.0;
            let current_put = Black76::price(current_price, pos.put_strike, time_to_expiry, 
                                             RISK_FREE_RATE, sim_config.volatility, false);
            let current_call = Black76::price(current_price, pos.call_strike, time_to_expiry,
                                              RISK_FREE_RATE, sim_config.volatility, true);
            let current_value = current_put + current_call;
            let entry_value = pos.put_entry_premium + pos.call_entry_premium;
            let unrealized_pnl = entry_value - current_value;
            
            println!("Day {} ({}): Price ${:.2} | Holding pos {} | DTE: {} | Unrealized P&L: ${:.2}",
                     day, date_str, current_price, pos.position_id.0, remaining_dte, unrealized_pnl);
        }
    }

    // Final summary
    println!("\n{}", "=".repeat(60));
    println!("SIMULATION SUMMARY");
    println!("{}", "=".repeat(60));
    println!("Total positions opened: {}", pnl_summary.position_count);
    println!("Total premium collected: ${:.2}", pnl_summary.total_premium_collected);
    println!("Total premium paid: ${:.2}", pnl_summary.total_premium_paid);
    println!("Net P&L: ${:.2}", pnl_summary.total_premium_collected - pnl_summary.total_premium_paid);
    println!("Final underlying price: ${:.2}", 
             price_path.last().map(|(_, p)| *p).unwrap_or(sim_config.initial_price));
}

/// Open a position with Black-76 pricing
fn open_position_with_pricing(
    calendar: &Calendar,
    event_store: &mut EventStore,
    pnl: &mut PnLSummary,
    entry_day: Day,
    entry_time: TimeOfDay,
    current_price: f64,
    volatility: f64,
) -> PositionTracking {
    let expiration_day = calendar.next_trading_day(entry_day);
    let time_to_expiry = calendar.calculate_dte(entry_day, expiration_day) as f64 / 252.0;
    
    let position_id = event_store.next_position_id();
    let put_leg_id = event_store.next_leg_id();
    let call_leg_id = event_store.next_leg_id();
    
    let put_strike = current_price;  // ATM
    let call_strike = current_price; // ATM
    
    // Price using Black-76
    let put_premium = Black76::price(current_price, put_strike, time_to_expiry, 
                                     RISK_FREE_RATE, volatility, false);
    let call_premium = Black76::price(current_price, call_strike, time_to_expiry,
                                      RISK_FREE_RATE, volatility, true);
    
    // Calculate Greeks
    let put_greeks = Black76::greeks(current_price, put_strike, time_to_expiry,
                                     RISK_FREE_RATE, volatility, false);
    let call_greeks = Black76::greeks(current_price, call_strike, time_to_expiry,
                                      RISK_FREE_RATE, volatility, true);
    
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
    
    let event = Event::PositionOpened {
        position_id,
        timestamp: (entry_day, entry_time),
        legs: vec![
            (put_leg_id, put_contract, put_premium),
            (call_leg_id, call_contract, call_premium),
        ],
    };
    event_store.append(event);
    
    pnl.position_count += 1;
    pnl.total_premium_collected += put_premium + call_premium;
    
    PositionTracking {
        position_id,
        entry_day,
        expiration_day,
        entry_price: current_price,
        put_strike,
        call_strike,
        put_entry_premium: put_premium,
        call_entry_premium: call_premium,
        put_greeks,
        call_greeks,
    }
}

/// Calculate close value (intrinsic at expiry)
fn calculate_close_value(underlying: f64, strike: f64, is_call: bool) -> f64 {
    if is_call {
        (underlying - strike).max(0.0)
    } else {
        (strike - underlying).max(0.0)
    }
}

/// Print Greeks for a position
fn print_greeks(pos: &PositionTracking) {
    let total_delta = pos.put_greeks.delta + pos.call_greeks.delta;
    let total_gamma = pos.put_greeks.gamma + pos.call_greeks.gamma;
    let total_theta = pos.put_greeks.theta + pos.call_greeks.theta;
    let total_vega = pos.put_greeks.vega + pos.call_greeks.vega;
    
    println!("      Greeks: δ={:.3} γ={:.4} θ={:.3}/day ν={:.3}",
             total_delta, total_gamma, total_theta, total_vega);
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
