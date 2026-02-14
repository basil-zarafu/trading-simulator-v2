//! Trading Simulator V2 - Intraday Version (10-minute resolution)
//!
//! Supports:
//! - 10-minute granularity (matching V1)
//! - 23/5 trading calendar (/CL futures)
//! - Intraday roll triggers (profit targets, DTE, time-based)
//! - Fractional DTE calculation
//! - Event sourcing with precise timestamps
//!
//! Usage:
//!   cargo run -- config/straddle_1dte.yaml
//!   cargo run -- config/long_protection.yaml

mod calendar;
mod config;
mod events;
mod prices;
mod pricing;
mod triggers;

use calendar::intraday::{TradingCalendar, Timestamp};
use config::Config;
use events::{CloseReason, Event, EventStore, LegId, OptionContract, OptionType, PositionId, Side};
use prices::{GBM, PricePoint};
use pricing::{Black76, Greeks};
use std::env;

/// Parse time string "HH:MM" to minutes from midnight
fn parse_time(time_str: &str) -> u32 {
    let parts: Vec<&str> = time_str.split(':').collect();
    let hours: u32 = parts[0].parse().unwrap_or(14);
    let minutes: u32 = parts[1].parse().unwrap_or(0);
    hours * 60 + minutes
}

/// Position tracking with P&L (intraday version)
#[derive(Debug)]
struct PositionTracking {
    position_id: PositionId,
    entry_timestamp: Timestamp,
    expiration_day: u32,
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
    println!("Trading Simulator V2 - Intraday Version (10-minute resolution)\n");

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

    // Setup trading calendar and price generator
    let calendar = TradingCalendar::new();
    let mut event_store = EventStore::new();

    // Generate intraday price path
    let start_day = 0; // Day 0 = Monday
    let start_minute = 9 * 60; // 9:00 AM
    
    let mut gbm = GBM::new(
        config.simulation.initial_price,
        config.simulation.drift,
        config.simulation.volatility,
        config.simulation.seed,
    );
    
    let resolution = config.simulation.intraday_resolution_minutes;
    let price_bars = gbm.generate_intraday_path(
        &calendar,
        config.simulation.days,
        resolution,
        start_day,
        start_minute,
    );

    // Calculate implied volatility for option pricing
    let realized_vol = config.simulation.volatility;
    let implied_vol = realized_vol + config.simulation.volatility_risk_premium;
    
    // Print configuration
    println!("Simulation Parameters:");
    println!("  Days: {}", config.simulation.days);
    println!("  Resolution: {} minutes", config.simulation.intraday_resolution_minutes);
    println!("  Total bars: {}", price_bars.len());
    println!("  Initial price: ${:.2}", config.simulation.initial_price);
    println!("  Drift (μ): {:.2}%", config.simulation.drift * 100.0);
    println!("  Realized volatility: {:.0}%", realized_vol * 100.0);
    println!("  Volatility Risk Premium: {:.1}%", config.simulation.volatility_risk_premium * 100.0);
    println!("  Implied volatility: {:.0}% (for option pricing)", implied_vol * 100.0);
    println!("  Risk-free rate: {:.1}%", config.simulation.risk_free_rate * 100.0);
    println!("  Seed: {}", config.simulation.seed);
    println!();
    println!("Strategy: {} ({} DTE)", config.strategy.strategy_type, config.strategy.entry_dte);
    println!("  Side: {} ({})", 
        config.strategy.side,
        if config.strategy.side == "long" { "pay premium" } else { "collect premium" }
    );
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

    // Run simulation bar by bar
    for price_point in &price_bars {
        let current_price = price_point.price;
        let timestamp = price_point.timestamp;
        let date_str = format_timestamp(&timestamp);

        // Check for roll triggers
        if let Some(pos) = active_position.take() {
            // Calculate fractional DTE
            let fractional_dte = calculate_fractional_dte(&timestamp, pos.expiration_day);
            
            // Check if we should roll (DTE threshold or time-based)
            let should_roll = if config.strategy.entry_dte == 1 {
                // For 1DTE: roll at roll_time on expiration day
                timestamp.day == pos.expiration_day && timestamp.minute >= roll_time
            } else {
                // For longer DTE: roll when DTE <= 28
                fractional_dte <= 28.0
            };
            
            if should_roll {
                // Close current position
                let (put_close, call_close) = if fractional_dte > 0.0 {
                    // Early close: use Black76 to include time value
                    let time_to_expiry = fractional_dte / 252.0;
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
                    // Expiration: use intrinsic value only
                    let put = calculate_intrinsic(current_price, pos.put_strike, false);
                    let call = calculate_intrinsic(current_price, pos.call_strike, true);
                    (put, call)
                };
                
                // Calculate P&L based on position side
                let is_long = config.strategy.side == "long";
                let position_pnl = if is_long {
                    // Long: Close Value - Entry Premium
                    (put_close + call_close) - (pos.put_entry_premium + pos.call_entry_premium)
                } else {
                    // Short: Entry Premium - Close Value
                    (pos.put_entry_premium + pos.call_entry_premium) - (put_close + call_close)
                };
                let position_pnl_dollars = position_pnl * config.simulation.contract_multiplier;
                
                // Track close value
                if is_long {
                    pnl_summary.total_premium_collected += put_close + call_close;
                } else {
                    pnl_summary.total_premium_paid += put_close + call_close;
                }
                
                let reason_str = if fractional_dte <= 0.0 { "Expiration" } else { "Roll" };
                print!("{} | Price ${:.2} | ", date_str, current_price);
                println!(
                    "CLOSED position {} at {} | P&L: ${:.0} ({})",
                    pos.position_id.0,
                    &config.strategy.roll_time,
                    position_pnl_dollars,
                    reason_str
                );
                
                let close_event = Event::PositionClosed {
                    position_id: pos.position_id,
                    timestamp: (timestamp.day, timestamp.minute as u16),
                    close_premiums: vec![
                        (LegId(pos.position_id.0 * 2 - 1), put_close),
                        (LegId(pos.position_id.0 * 2), call_close),
                    ],
                    reason: CloseReason::Expiration,
                };
                event_store.append(close_event);
                
                // Open new position at roll time
                let use_same_strikes = config.strike_config.roll_type == "same_strikes";
                let new_pos = open_position_with_pricing(
                    &calendar,
                    &mut event_store,
                    &mut pnl_summary,
                    &config,
                    timestamp.day,
                    roll_time,
                    current_price,
                    if use_same_strikes {
                        Some((pos.put_strike, pos.call_strike))
                    } else {
                        None
                    },
                    implied_vol,
                );
                let new_total = new_pos.put_entry_premium + new_pos.call_entry_premium;
                let new_total_dollars = new_total * config.simulation.contract_multiplier;
                let new_display_premium = if is_long { -new_total } else { new_total };
                let new_display_premium_dollars = if is_long { -new_total_dollars } else { new_total_dollars };
                let roll_type_str = if use_same_strikes { " (same strikes)" } else { "" };
                println!(
                    "  -> OPENED position {} at {} | Strikes: Put ${:.2} Call ${:.2} | ${:.2} per barrel (${:.0} total){}",
                    new_pos.position_id.0,
                    &config.strategy.roll_time,
                    new_pos.put_strike,
                    new_pos.call_strike,
                    new_display_premium,
                    new_display_premium_dollars,
                    roll_type_str
                );
                print_greeks(&new_pos);
                
                active_position = Some(new_pos);
                continue;
            } else {
                // No roll triggered, keep position
                active_position = Some(pos);
            }
        }

        // Open new position at entry time if none exists
        if active_position.is_none() && timestamp.minute >= entry_time {
            let pos = open_position_with_pricing(
                &calendar,
                &mut event_store,
                &mut pnl_summary,
                &config,
                timestamp.day,
                entry_time,
                current_price,
                None,
                implied_vol,
            );

            let is_long = config.strategy.side == "long";
            let total_premium = pos.put_entry_premium + pos.call_entry_premium;
            let total_premium_dollars = total_premium * config.simulation.contract_multiplier;
            let display_premium = if is_long { -total_premium } else { total_premium };
            let display_premium_dollars = if is_long { -total_premium_dollars } else { total_premium_dollars };
            
            print!("{} | Price ${:.2} | ", date_str, current_price);
            println!(
                "OPENED position {} at {} | Strikes: Put ${:.2} Call ${:.2} | ${:.2} per barrel (${:.0} total)",
                pos.position_id.0,
                &config.strategy.entry_time,
                pos.put_strike,
                pos.call_strike,
                display_premium,
                display_premium_dollars
            );
            print_greeks(&pos);

            active_position = Some(pos);
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
    if let Some(last_point) = price_bars.last() {
        println!("Final underlying price: ${:.2}", last_point.price);
    }
}

/// Calculate fractional days to expiration
fn calculate_fractional_dte(current: &Timestamp, expiration_day: u32) -> f64 {
    if current.day >= expiration_day {
        return 0.0;
    }
    // Approximate: each day is 1.0, each minute is 1/138 (for 23-hour trading day at 10-min bars)
    let days_remaining = (expiration_day - current.day) as f64;
    let minutes_fraction = (138.0 - current.minute as f64) / 138.0;
    days_remaining - 1.0 + minutes_fraction
}

/// Format timestamp as human-readable string
fn format_timestamp(ts: &Timestamp) -> String {
    let hours = ts.minute / 60;
    let mins = ts.minute % 60;
    let weekday = match ts.day % 7 {
        0 => "Mon", 1 => "Tue", 2 => "Wed", 3 => "Thu",
        4 => "Fri", 5 => "Sat", 6 => "Sun", _ => "???",
    };
    let week = ts.day / 7;
    format!("Day {} ({} W{}) {:02}:{:02}", ts.day, weekday, week, hours, mins)
}

/// Calculate intrinsic value at expiration
fn calculate_intrinsic(underlying: f64, strike: f64, is_call: bool) -> f64 {
    if is_call {
        (underlying - strike).max(0.0)
    } else {
        (strike - underlying).max(0.0)
    }
}

/// Open a position with Black-76 pricing
fn open_position_with_pricing(
    calendar: &TradingCalendar,
    event_store: &mut EventStore,
    pnl: &mut PnLSummary,
    config: &Config,
    entry_day: u32,
    entry_time: u32,
    current_price: f64,
    strike_override: Option<(f64, f64)>,
    implied_vol: f64,
) -> PositionTracking {
    // Calculate expiration day based on entry_dte config
    let mut expiration_day = entry_day;
    let mut trading_days_count = 0;
    let calendar_old = calendar::Calendar::new();
    while trading_days_count < config.strategy.entry_dte {
        expiration_day = calendar_old.next_trading_day(expiration_day);
        trading_days_count += 1;
    }
    let time_to_expiry = config.strategy.entry_dte as f64 / 252.0;

    let position_id = event_store.next_position_id();
    let put_leg_id = event_store.next_leg_id();
    let call_leg_id = event_store.next_leg_id();

    // Determine strikes
    let (put_strike, call_strike) = if let Some((put, call)) = strike_override {
        (put, call)
    } else {
        match config.strategy.strike_selection.as_str() {
            "OTM" => {
                let offset = config.strategy.strike_offset;
                let atm = config.strike_config.round_to_strike(current_price);
                let put = config.strike_config.round_to_strike(atm - offset);
                let call = config.strike_config.round_to_strike(atm + offset);
                (put, call)
            }
            _ => {
                let atm = config.strike_config.round_to_strike(current_price);
                (atm, atm)
            }
        }
    };

    // Price using Black-76 with IMPLIED volatility
    let put_premium = Black76::price(
        current_price, put_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, false
    );
    let call_premium = Black76::price(
        current_price, call_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, true
    );

    // Calculate Greeks
    let put_greeks = Black76::greeks(
        current_price, put_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, false
    );
    let call_greeks = Black76::greeks(
        current_price, call_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, true
    );

    // Determine side
    let side = if config.strategy.side == "long" { Side::Long } else { Side::Short };
    
    let put_contract = OptionContract {
        underlying_price: current_price,
        strike: put_strike,
        option_type: OptionType::Put,
        side,
        expiration_day,
    };

    let call_contract = OptionContract {
        underlying_price: current_price,
        strike: call_strike,
        option_type: OptionType::Call,
        side,
        expiration_day,
    };

    let put_premium_signed = if side == Side::Long { -put_premium } else { put_premium };
    let call_premium_signed = if side == Side::Long { -call_premium } else { call_premium };
    
    let event = Event::PositionOpened {
        position_id,
        timestamp: (entry_day, entry_time as u16),
        legs: vec![
            (put_leg_id, put_contract, put_premium_signed),
            (call_leg_id, call_contract, call_premium_signed),
        ],
    };
    event_store.append(event);

    pnl.position_count += 1;
    if side == Side::Short {
        pnl.total_premium_collected += put_premium + call_premium;
    } else {
        pnl.total_premium_paid += put_premium + call_premium;
    }

    PositionTracking {
        position_id,
        entry_timestamp: Timestamp::new(entry_day, entry_time),
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
