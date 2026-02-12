//! Trading Simulator V2 - Phase 4: YAML Configuration
//!
//! Demonstrates:
//! - Synthetic calendar
//! - Event sourcing
//! - GBM price generation
//! - Black-76 option pricing
//! - P&L tracking through position lifecycle
//! - YAML configuration (no code changes needed)
//!
//! Usage:
//!   cargo run -- config/straddle_1dte.yaml
//!   cargo run -- config/long_protection.yaml

mod calendar;
mod config;
mod events;
mod prices;
mod pricing;

use calendar::{Calendar, Day, TimeOfDay};
use config::Config;
use events::{CloseReason, Event, EventStore, LegId, OptionContract, OptionType, PositionId, Side};
use prices::GBM;
use pricing::{Black76, Greeks};
use std::env;

/// Parse time string "HH:MM" to minutes from midnight
fn parse_time(time_str: &str) -> TimeOfDay {
    let parts: Vec<&str> = time_str.split(':').collect();
    let hours: TimeOfDay = parts[0].parse().unwrap_or(14);
    let minutes: TimeOfDay = parts[1].parse().unwrap_or(0);
    hours * 60 + minutes
}

/// Position tracking with P&L
#[derive(Debug)]
struct PositionTracking {
    position_id: PositionId,
    expiration_day: Day,
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
    println!("Trading Simulator V2 - Phase 4: YAML Configuration\n");

    // Load configuration from file or use default
    let config = match env::args().nth(1) {
        Some(path) => {
            println!("Loading configuration from: {}", path);
            match Config::from_file(&path) {
                Ok(cfg) => {
                    println!("✓ Configuration loaded successfully\n");
                    cfg
                }
                Err(e) => {
                    eprintln!("✗ Failed to load config: {}", e);
                    eprintln!("Using default 1DTE straddle configuration\n");
                    Config::default_1dte_straddle()
                }
            }
        }
        None => {
            println!("Usage: cargo run -- <config.yaml>");
            println!("Using default 1DTE straddle configuration\n");
            Config::default_1dte_straddle()
        }
    };

    // Parse times from config
    let entry_time = parse_time(&config.strategy.entry_time);
    let roll_time = parse_time(&config.strategy.roll_time);

    let calendar = Calendar::new();
    let mut event_store = EventStore::new();

    // Generate price path using GBM
    let mut gbm = GBM::new(
        config.simulation.initial_price,
        config.simulation.drift,
        config.simulation.volatility,
        config.simulation.seed,
    );
    let price_path = gbm.generate_path(config.simulation.days);

    // Print configuration
    println!("Simulation Parameters:");
    println!("  Days: {}", config.simulation.days);
    println!("  Initial price: ${:.2}", config.simulation.initial_price);
    println!("  Drift (μ): {:.2}%", config.simulation.drift * 100.0);
    println!("  Volatility (σ): {:.0}%", config.simulation.volatility * 100.0);
    println!("  Risk-free rate: {:.1}%", config.simulation.risk_free_rate * 100.0);
    println!("  Seed: {}", config.simulation.seed);
    println!();
    println!("Strategy: {} ({} DTE)", config.strategy.strategy_type, config.strategy.entry_dte);
    println!("  Entry time: {}", config.strategy.entry_time);
    println!("  Roll time: {}", config.strategy.roll_time);
    println!("  Strike selection: {}", config.strategy.strike_selection);
    println!("  Strike tick size: ${:.2}", config.strike_config.tick_size);
    println!("  Roll type: {}", config.strike_config.roll_type);
    if config.strategy.strike_offset > 0.0 {
        println!("  Strike offset: {} points", config.strategy.strike_offset);
    }
    println!();

    // Track active position
    let mut active_position: Option<PositionTracking> = None;
    let mut pnl_summary = PnLSummary::default();

    // Run simulation day by day
    let mut price_iter = price_path.iter();

    for day in 0..config.simulation.days as u32 {
        if !calendar.is_trading_day(day) {
            continue;
        }

        let current_price = price_iter
            .next()
            .map(|(_, price)| *price)
            .unwrap_or_else(|| {
                price_path
                    .last()
                    .map(|(_, p)| *p)
                    .unwrap_or(config.simulation.initial_price)
            });

        let date_str = format_day(day);

        // Check for roll trigger
        if let Some(pos) = active_position.take() {
            if day == pos.expiration_day {
                // Calculate close value (intrinsic at expiry)
                let put_close = calculate_close_value(current_price, pos.put_strike, false);
                let call_close = calculate_close_value(current_price, pos.call_strike, true);

                let position_pnl =
                    (pos.put_entry_premium + pos.call_entry_premium) - (put_close + call_close);
                let position_pnl_dollars = position_pnl * config.simulation.contract_multiplier;
                pnl_summary.total_premium_paid += put_close + call_close;

                print!("Day {} ({}): Price ${:.2} | ", day, date_str, current_price);
                println!(
                    "CLOSED position {} at {} | P&L: ${:.0}",
                    pos.position_id.0,
                    &config.strategy.roll_time,
                    position_pnl_dollars
                );

                let close_event = Event::PositionClosed {
                    position_id: pos.position_id,
                    timestamp: (day, roll_time),
                    close_premiums: vec![
                        (LegId(pos.position_id.0 * 2 - 1), put_close),
                        (LegId(pos.position_id.0 * 2), call_close),
                    ],
                    reason: CloseReason::Expiration,
                };
                event_store.append(close_event);

                // Open new position - check roll type
                let use_same_strikes = config.strike_config.roll_type == "same_strikes";
                let new_pos = open_position_with_pricing(
                    &calendar,
                    &mut event_store,
                    &mut pnl_summary,
                    &config,
                    day,
                    roll_time,
                    current_price,
                    if use_same_strikes {
                        Some((pos.put_strike, pos.call_strike))
                    } else {
                        None
                    },
                );
                let new_total = new_pos.put_entry_premium + new_pos.call_entry_premium;
                let new_total_dollars = new_total * config.simulation.contract_multiplier;
                let roll_type_str = if use_same_strikes { " (same strikes)" } else { "" };
                println!(
                    "  -> OPENED position {} at {} | Strikes: Put ${:.2} Call ${:.2} | ${:.2} per barrel (${:.0} total){}",
                    new_pos.position_id.0,
                    &config.strategy.roll_time,
                    new_pos.put_strike,
                    new_pos.call_strike,
                    new_total,
                    new_total_dollars,
                    roll_type_str
                );
                print_greeks(&new_pos);

                active_position = Some(new_pos);
                continue;
            }
        }

        // Open new position
        if active_position.is_none() {
            let pos = open_position_with_pricing(
                &calendar,
                &mut event_store,
                &mut pnl_summary,
                &config,
                day,
                entry_time,
                current_price,
                None, // No strike override for new positions
            );

            let total_premium = pos.put_entry_premium + pos.call_entry_premium;
            let total_premium_dollars = total_premium * config.simulation.contract_multiplier;
            print!("Day {} ({}): Price ${:.2} | ", day, date_str, current_price);
            println!(
                "OPENED position {} at {} | Strikes: Put ${:.2} Call ${:.2} | ${:.2} per barrel (${:.0} total)",
                pos.position_id.0,
                &config.strategy.entry_time,
                pos.put_strike,
                pos.call_strike,
                total_premium,
                total_premium_dollars
            );
            print_greeks(&pos);

            active_position = Some(pos);
        } else {
            // Just holding - show unrealized P&L
            let pos = active_position.as_ref().unwrap();
            let remaining_dte = calendar.calculate_dte(day, pos.expiration_day);

            let time_to_expiry = remaining_dte as f64 / 252.0;
            let current_put = Black76::price(
                current_price,
                pos.put_strike,
                time_to_expiry,
                config.simulation.risk_free_rate,
                config.simulation.volatility,
                false,
            );
            let current_call = Black76::price(
                current_price,
                pos.call_strike,
                time_to_expiry,
                config.simulation.risk_free_rate,
                config.simulation.volatility,
                true,
            );
            let current_value = current_put + current_call;
            let entry_value = pos.put_entry_premium + pos.call_entry_premium;
            let unrealized_pnl = entry_value - current_value;
            let unrealized_pnl_dollars = unrealized_pnl * config.simulation.contract_multiplier;

            println!(
                "Day {} ({}): Price ${:.2} | Holding pos {} | DTE: {} | Unrealized P&L: ${:.0}",
                day, date_str, current_price, pos.position_id.0, remaining_dte, unrealized_pnl_dollars
            );
        }
    }

    // Final summary
    println!("\n{}", "=".repeat(60));
    println!("SIMULATION SUMMARY");
    println!("{}", "=".repeat(60));
    println!("Total positions opened: {}", pnl_summary.position_count);
    println!(
        "Total premium collected: ${:.2} per barrel (${:.0} total)",
        pnl_summary.total_premium_collected,
        pnl_summary.total_premium_collected * config.simulation.contract_multiplier
    );
    println!(
        "Total premium paid: ${:.2} per barrel (${:.0} total)",
        pnl_summary.total_premium_paid,
        pnl_summary.total_premium_paid * config.simulation.contract_multiplier
    );
    let net_pnl = pnl_summary.total_premium_collected - pnl_summary.total_premium_paid;
    println!(
        "Net P&L: ${:.2} per barrel (${:.0} total)",
        net_pnl,
        net_pnl * config.simulation.contract_multiplier
    );
    println!(
        "Contract multiplier: {} barrels",
        config.simulation.contract_multiplier as u32
    );
    println!(
        "Final underlying price: ${:.2}",
        price_path
            .last()
            .map(|(_, p)| *p)
            .unwrap_or(config.simulation.initial_price)
    );
}

/// Open a position with Black-76 pricing
/// 
/// If `strike_override` is Some((put, call)), use those strikes (for same_strikes roll type).
/// Otherwise, calculate strikes based on config (ATM or OTM).
fn open_position_with_pricing(
    calendar: &Calendar,
    event_store: &mut EventStore,
    pnl: &mut PnLSummary,
    config: &Config,
    entry_day: Day,
    entry_time: TimeOfDay,
    current_price: f64,
    strike_override: Option<(f64, f64)>,
) -> PositionTracking {
    let expiration_day = calendar.next_trading_day(entry_day);
    let time_to_expiry = calendar.calculate_dte(entry_day, expiration_day) as f64 / 252.0;

    let position_id = event_store.next_position_id();
    let put_leg_id = event_store.next_leg_id();
    let call_leg_id = event_store.next_leg_id();

    // Determine strikes
    let (put_strike, call_strike) = if let Some((put, call)) = strike_override {
        // Use specified strikes (for same_strikes roll type)
        (put, call)
    } else {
        // Calculate strikes based on configuration
        match config.strategy.strike_selection.as_str() {
            "OTM" => {
                let offset = config.strategy.strike_offset;
                let atm = config.strike_config.round_to_strike(current_price);
                let put = config.strike_config.round_to_strike(atm - offset);
                let call = config.strike_config.round_to_strike(atm + offset);
                (put, call)
            }
            selection if selection.starts_with("delta_") => {
                // Delta-based strike selection
                // Format: "delta_put_30" or "delta_call_16"
                let parts: Vec<&str> = selection.split('_').collect();
                if parts.len() >= 3 {
                    let option_type = parts[1]; // "put" or "call"
                    let target_delta: f64 = parts[2].parse().unwrap_or(30.0) / 100.0;
                    
                    let strike = find_strike_by_delta(
                        current_price,
                        time_to_expiry,
                        config.simulation.risk_free_rate,
                        config.simulation.volatility,
                        option_type == "call",
                        target_delta,
                        &config.strike_config,
                    );
                    
                    if option_type == "put" {
                        (strike, config.strike_config.round_to_strike(current_price))
                    } else {
                        (config.strike_config.round_to_strike(current_price), strike)
                    }
                } else {
                    // Fallback to ATM
                    let atm = config.strike_config.round_to_strike(current_price);
                    (atm, atm)
                }
            }
            _ => {
                // ATM - round to nearest valid strike
                let atm = config.strike_config.round_to_strike(current_price);
                (atm, atm)
            }
        }
    };

    // Price using Black-76
    let put_premium = Black76::price(
        current_price,
        put_strike,
        time_to_expiry,
        config.simulation.risk_free_rate,
        config.simulation.volatility,
        false,
    );
    let call_premium = Black76::price(
        current_price,
        call_strike,
        time_to_expiry,
        config.simulation.risk_free_rate,
        config.simulation.volatility,
        true,
    );

    // Calculate Greeks
    let put_greeks = Black76::greeks(
        current_price,
        put_strike,
        time_to_expiry,
        config.simulation.risk_free_rate,
        config.simulation.volatility,
        false,
    );
    let call_greeks = Black76::greeks(
        current_price,
        call_strike,
        time_to_expiry,
        config.simulation.risk_free_rate,
        config.simulation.volatility,
        true,
    );

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
        expiration_day,
        put_strike,
        call_strike,
        put_entry_premium: put_premium,
        call_entry_premium: call_premium,
        put_greeks,
        call_greeks,
    }
}

/// Find strike price closest to target delta
/// 
/// Searches through available strikes to find the one with delta closest to target.
/// For puts, target delta is typically negative (e.g., -0.30 for 30 delta put).
/// For calls, target delta is positive (e.g., 0.30 for 30 delta call).
fn find_strike_by_delta(
    underlying: f64,
    time_to_expiry: f64,
    risk_free_rate: f64,
    volatility: f64,
    is_call: bool,
    target_delta: f64,
    strike_config: &config::StrikeConfig,
) -> f64 {
    let atm = strike_config.round_to_strike(underlying);
    let mut best_strike = atm;
    let mut best_delta_diff = f64::INFINITY;
    
    // Search up to 20 strikes in each direction
    for i in -20..=20 {
        let strike = atm + (i as f64) * strike_config.tick_size;
        if strike <= 0.0 {
            continue;
        }
        
        let greeks = Black76::greeks(
            underlying,
            strike,
            time_to_expiry,
            risk_free_rate,
            volatility,
            is_call,
        );
        
        // For puts, we want delta close to -target (e.g., -0.30)
        // For calls, we want delta close to +target (e.g., 0.30)
        let target = if is_call { target_delta } else { -target_delta };
        let delta_diff = (greeks.delta - target).abs();
        
        if delta_diff < best_delta_diff {
            best_delta_diff = delta_diff;
            best_strike = strike;
        }
    }
    
    best_strike
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

    println!(
        "      Greeks: δ={:.3} γ={:.4} θ={:.3}/day ν={:.3}",
        total_delta, total_gamma, total_theta, total_vega
    );
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
