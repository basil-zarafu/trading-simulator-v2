//! Combined Strategy Runner
//! 
//! Runs both short and long legs simultaneously with the same price path
//! Usage: cargo run --bin combined -- config/combined.yaml

mod calendar;
mod config;
mod events;
mod prices;
mod pricing;
mod triggers;

use calendar::{Calendar, Day, TimeOfDay};
use config::{Config, StrategyConfig};
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
    entry_day: Day,
    expiration_day: Day,
    entry_price: f64,
    put_strike: f64,
    call_strike: f64,
    put_entry_premium: f64,
    call_entry_premium: f64,
}

/// P&L summary for a leg
#[derive(Debug, Default)]
struct LegPnL {
    total_premium_collected: f64,
    total_premium_paid: f64,
    position_count: u32,
    net_pnl: f64,
}

/// Combined P&L tracking
#[derive(Debug, Default)]
struct CombinedPnL {
    short: LegPnL,
    long: LegPnL,
}

fn main() {
    println!("Trading Simulator V2 - Combined Strategy Runner\n");

    // Load configuration
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
                    std::process::exit(1);
                }
            }
        }
        None => {
            println!("Usage: cargo run --bin combined -- <config.yaml>");
            std::process::exit(1);
        }
    };

    // Check if this is a combined strategy
    let has_short = config.short_leg.as_ref().map(|s| s.enabled).unwrap_or(false);
    let has_long = config.long_leg.as_ref().map(|s| s.enabled).unwrap_or(false);
    
    if !has_short && !has_long {
        eprintln!("Error: No enabled legs found in config");
        eprintln!("Please enable short_leg and/or long_leg in the YAML file");
        std::process::exit(1);
    }

    // Print configuration
    println!("Simulation Parameters:");
    println!("  Days: {}", config.simulation.days);
    println!("  Initial price: ${:.2}", config.simulation.initial_price);
    println!("  Volatility: {:.0}%", config.simulation.volatility * 100.0);
    println!("  VRP: {:.1}%", config.simulation.volatility_risk_premium * 100.0);
    println!("  Seed: {}", config.simulation.seed);
    println!();

    let realized_vol = config.simulation.volatility;
    let implied_vol = realized_vol + config.simulation.volatility_risk_premium;

    // Generate single price path (shared by both legs)
    let mut gbm = GBM::new(
        config.simulation.initial_price,
        config.simulation.drift,
        config.simulation.volatility,
        config.simulation.seed,
    );
    let price_path = gbm.generate_path(config.simulation.days);

    // Run both legs
    let mut combined_pnl = CombinedPnL::default();
    
    if has_short {
        println!("=== SHORT LEG (1DTE Straddle) ===");
        let short_config = config.short_leg.as_ref().unwrap();
        combined_pnl.short = run_leg(
            &config, &price_path, short_config, implied_vol, "SHORT"
        );
        println!();
    }

    if has_long {
        println!("=== LONG LEG (70DTE Protection) ===");
        let long_config = config.long_leg.as_ref().unwrap();
        combined_pnl.long = run_leg(
            &config, &price_path, long_config, implied_vol, "LONG"
        );
        println!();
    }

    // Print combined summary
    println!("{}", "=".repeat(60));
    println!("COMBINED STRATEGY SUMMARY");
    println!("{}", "=".repeat(60));
    
    let short_pnl = combined_pnl.short.net_pnl * config.simulation.contract_multiplier;
    let long_pnl = combined_pnl.long.net_pnl * config.simulation.contract_multiplier;
    let total_pnl = short_pnl + long_pnl;
    
    let days = config.simulation.days as f64;
    
    println!("Short Leg:");
    println!("  Positions: {}", combined_pnl.short.position_count);
    println!("  Net P&L: ${:.0}", short_pnl);
    println!("  P&L/Day: ${:.0}", short_pnl / days);
    
    println!("Long Leg:");
    println!("  Positions: {}", combined_pnl.long.position_count);
    println!("  Net P&L: ${:.0}", long_pnl);
    println!("  P&L/Day: ${:.0}", long_pnl / days);
    
    println!("Total:");
    println!("  Net P&L: ${:.0}", total_pnl);
    println!("  P&L/Day: ${:.0}", total_pnl / days);
    println!();
    println!("Final price: ${:.2}", price_path.last().map(|(_, p)| *p).unwrap_or(config.simulation.initial_price));
}

/// Run a single leg of the strategy
fn run_leg(
    config: &Config,
    price_path: &[(u32, f64)],
    leg_config: &StrategyConfig,
    implied_vol: f64,
    leg_name: &str,
) -> LegPnL {
    let calendar = Calendar::new();
    let mut pnl = LegPnL::default();
    
    let entry_time = parse_time(&leg_config.entry_time);
    let roll_time = parse_time(&leg_config.roll_time);
    let is_long = leg_config.side == "long";
    
    let mut active_position: Option<PositionTracking> = None;
    let mut position_id_counter = 1u64;
    
    for (day, current_price) in price_path.iter().copied() {
        if !calendar.is_trading_day(day) {
            continue;
        }

        // Check for roll triggers
        if let Some(pos) = active_position.take() {
            let remaining_dte = calendar.calculate_dte(day, pos.expiration_day);
            
            // Check DTE trigger
            let should_roll = remaining_dte as f64 <= 28.0;
            
            // For 1DTE, also check time trigger on expiration day
            let is_1dte = leg_config.entry_dte == 1;
            let time_trigger = is_1dte && day == pos.expiration_day;
            
            if should_roll || time_trigger {
                // Close position
                let (put_close, call_close) = if remaining_dte > 0 {
                    let time_to_expiry = remaining_dte as f64 / 252.0;
                    let put = Black76::price(
                        current_price, pos.put_strike, time_to_expiry,
                        config.simulation.risk_free_rate, implied_vol, false
                    );
                    let call = Black76::price(
                        current_price, pos.call_strike, time_to_expiry,
                        config.simulation.risk_free_rate, implied_vol, true
                    );
                    (put, call)
                } else {
                    let put = calculate_close_value(current_price, pos.put_strike, false);
                    let call = calculate_close_value(current_price, pos.call_strike, true);
                    (put, call)
                };
                
                let close_value = put_close + call_close;
                let entry_value = pos.put_entry_premium + pos.call_entry_premium;
                
                let position_pnl = if is_long {
                    close_value - entry_value
                } else {
                    entry_value - close_value
                };
                
                pnl.net_pnl += position_pnl;
                
                if is_long {
                    pnl.total_premium_collected += close_value;
                } else {
                    pnl.total_premium_paid += close_value;
                }
                
                let pnl_dollars = position_pnl * config.simulation.contract_multiplier;
                let reason = if time_trigger { "TimeTrigger" } else { "DteThreshold" };
                println!("[{}] Day {}: CLOSED position {} | P&L: ${:.0} ({})",
                    leg_name, day, pos.position_id.0, pnl_dollars, reason);
                
                // Open new position
                let new_pos = open_position(
                    &config, &calendar, &mut position_id_counter,
                    day, roll_time, current_price, implied_vol, leg_config
                );
                
                let total = new_pos.put_entry_premium + new_pos.call_entry_premium;
                let total_dollars = total * config.simulation.contract_multiplier;
                let display_total = if is_long { -total } else { total };
                let display_dollars = if is_long { -total_dollars } else { total_dollars };
                
                println!("[{}] Day {}: OPENED position {} | Strikes: P${:.2} C${:.2} | ${:.2} (${:.0})",
                    leg_name, day, new_pos.position_id.0,
                    new_pos.put_strike, new_pos.call_strike,
                    display_total, display_dollars);
                
                if is_long {
                    pnl.total_premium_paid += total;
                } else {
                    pnl.total_premium_collected += total;
                }
                pnl.position_count += 1;
                
                active_position = Some(new_pos);
            } else {
                active_position = Some(pos);
            }
        }

        // Open new position if none exists
        if active_position.is_none() {
            let pos = open_position(
                &config, &calendar, &mut position_id_counter,
                day, entry_time, current_price, implied_vol, leg_config
            );
            
            let total = pos.put_entry_premium + pos.call_entry_premium;
            let total_dollars = total * config.simulation.contract_multiplier;
            let display_total = if is_long { -total } else { total };
            let display_dollars = if is_long { -total_dollars } else { total_dollars };
            
            println!("[{}] Day {}: OPENED position {} | Strikes: P${:.2} C${:.2} | ${:.2} (${:.0})",
                leg_name, day, pos.position_id.0,
                pos.put_strike, pos.call_strike,
                display_total, display_dollars);
            
            if is_long {
                pnl.total_premium_paid += total;
            } else {
                pnl.total_premium_collected += total;
            }
            pnl.position_count += 1;
            
            active_position = Some(pos);
        }
    }
    
    pnl
}

/// Open a new position
fn open_position(
    config: &Config,
    calendar: &Calendar,
    position_id_counter: &mut u64,
    entry_day: Day,
    entry_time: TimeOfDay,
    current_price: f64,
    implied_vol: f64,
    leg_config: &StrategyConfig,
) -> PositionTracking {
    let mut expiration_day = entry_day;
    let mut trading_days_count = 0;
    while trading_days_count < leg_config.entry_dte {
        expiration_day = calendar.next_trading_day(expiration_day);
        trading_days_count += 1;
    }
    
    let time_to_expiry = leg_config.entry_dte as f64 / 252.0;
    
    let position_id = PositionId(*position_id_counter);
    *position_id_counter += 1;
    
    // Calculate strikes
    let (put_strike, call_strike) = match leg_config.strike_selection.as_str() {
        "OTM" => {
            let offset = leg_config.strike_offset;
            let atm = config.strike_config.round_to_strike(current_price);
            let put = config.strike_config.round_to_strike(atm - offset);
            let call = config.strike_config.round_to_strike(atm + offset);
            (put, call)
        }
        _ => {
            let atm = config.strike_config.round_to_strike(current_price);
            (atm, atm)
        }
    };
    
    let put_premium = Black76::price(
        current_price, put_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, false
    );
    let call_premium = Black76::price(
        current_price, call_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, true
    );
    
    PositionTracking {
        position_id,
        entry_day,
        expiration_day,
        entry_price: current_price,
        put_strike,
        call_strike,
        put_entry_premium: put_premium,
        call_entry_premium: call_premium,
    }
}

/// Calculate intrinsic value at expiration
fn calculate_close_value(underlying: f64, strike: f64, is_call: bool) -> f64 {
    if is_call {
        (underlying - strike).max(0.0)
    } else {
        (strike - underlying).max(0.0)
    }
}
