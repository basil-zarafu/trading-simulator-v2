//! Trading Simulator V2 - Intraday Version with JSON Output
//!
//! Supports:
//! - 10-minute granularity (matching V1)
//! - 23/5 trading calendar (/CL futures)
//! - Intraday roll triggers (profit targets, DTE, time-based)
//! - Fractional DTE calculation
//! - JSON output for web UI integration
//!
//! Usage:
//!   cargo run --bin trading-simulator-v2 -- config/straddle_1dte.yaml
//!   cargo run --bin trading-simulator-v2 -- --json  (for JSON output mode)

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
use serde::{Deserialize, Serialize};
use std::env;

/// JSON output structure for web UI
#[derive(Serialize, Deserialize, Debug)]
struct SimulationOutput {
    /// Configuration used
    config: SimulationConfig,
    /// Raw 10-minute price points
    price_points: Vec<PricePointJson>,
    /// Aggregated daily OHLC data
    daily_ohlc: Vec<DailyOHLC>,
    /// Trade events
    trades: Vec<TradeEvent>,
    /// Summary statistics
    summary: Summary,
}

#[derive(Serialize, Deserialize, Debug)]
struct SimulationConfig {
    days: usize,
    resolution_minutes: u32,
    initial_price: f64,
    volatility: f64,
    vrp: f64,
    seed: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct PricePointJson {
    day: u32,
    minute: u32,
    price: f64,
}

#[derive(Serialize, Deserialize, Debug)]
struct DailyOHLC {
    day: u32,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

#[derive(Serialize, Deserialize, Debug)]
struct TradeEvent {
    trade_type: String,
    day: u32,
    minute: u32,
    price: f64,
    message: String,
    pnl: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Summary {
    total_positions: u32,
    total_premium_collected: f64,
    total_premium_paid: f64,
    net_pnl: f64,
    final_price: f64,
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
    let args: Vec<String> = env::args().collect();
    let json_mode = args.contains(&"--json".to_string());
    
    if !json_mode {
        println!("Trading Simulator V2 - Intraday Version (10-minute resolution)\n");
    }

    // Load configuration
    let config = match env::args().nth(1) {
        Some(path) if !path.starts_with("--") => {
            if !json_mode {
                println!("Loading configuration from: {}", path);
            }
            match Config::from_file(&path) {
                Ok(cfg) => {
                    if !json_mode {
                        println!("✓ Configuration loaded successfully\n");
                    }
                    cfg
                }
                Err(e) => {
                    if !json_mode {
                        eprintln!("✗ Failed to load config: {}", e);
                        eprintln!("Using default 1DTE straddle configuration\n");
                    }
                    Config::default_1dte_straddle()
                }
            }
        }
        _ => {
            if !json_mode {
                println!("Usage: cargo run --bin trading-simulator-v2 -- <config.yaml>");
                println!("Using default 1DTE straddle configuration\n");
            }
            Config::default_1dte_straddle()
        }
    };

    // Parse times
    let entry_time = parse_time(&config.strategy.entry_time);
    let roll_time = parse_time(&config.strategy.roll_time);

    // Setup
    let calendar = TradingCalendar::new();
    let mut event_store = EventStore::new();

    // Generate price path
    let start_day = 0;
    let start_minute = 9 * 60;
    
    let mut gbm = GBM::new(
        config.simulation.initial_price,
        config.simulation.drift,
        config.simulation.volatility,
        config.simulation.seed,
    );
    
    let resolution = config.simulation.intraday_resolution_minutes;
    let price_points = gbm.generate_intraday_path(
        &calendar,
        config.simulation.days,
        resolution,
        start_day,
        start_minute,
    );

    // Aggregate to daily OHLC
    let daily_ohlc = aggregate_to_daily_ohlc(&price_points);

    // Calculate implied vol
    let realized_vol = config.simulation.volatility;
    let implied_vol = realized_vol + config.simulation.volatility_risk_premium;

    // Print config (text mode)
    if !json_mode {
        println!("Simulation Parameters:");
        println!("  Days: {}", config.simulation.days);
        println!("  Resolution: {} minutes", config.simulation.intraday_resolution_minutes);
        println!("  Total points: {}", price_points.len());
        println!("  Initial price: ${:.2}", config.simulation.initial_price);
        println!("  Volatility: {:.0}%", realized_vol * 100.0);
        println!("  VRP: {:.1}%", config.simulation.volatility_risk_premium * 100.0);
        println!("  Seed: {}", config.simulation.seed);
        println!();
    }

    // Run simulation
    let mut active_position: Option<PositionTracking> = None;
    let mut pnl_summary = PnLSummary::default();
    let mut trades: Vec<TradeEvent> = Vec::new();

    for price_point in &price_points {
        let current_price = price_point.price;
        let timestamp = price_point.timestamp;
        let date_str = format_timestamp(&timestamp);

        // Check for roll triggers
        if let Some(pos) = active_position.take() {
            let fractional_dte = calculate_fractional_dte(&timestamp, pos.expiration_day);
            
            let should_roll = if config.strategy.entry_dte == 1 {
                timestamp.day == pos.expiration_day && timestamp.minute >= roll_time
            } else {
                fractional_dte <= 28.0
            };
            
            if should_roll {
                // Close position
                let (put_close, call_close) = if fractional_dte > 0.0 {
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
                    let put = calculate_intrinsic(current_price, pos.put_strike, false);
                    let call = calculate_intrinsic(current_price, pos.call_strike, true);
                    (put, call)
                };
                
                let is_long = config.strategy.side == "long";
                let position_pnl = if is_long {
                    (put_close + call_close) - (pos.put_entry_premium + pos.call_entry_premium)
                } else {
                    (pos.put_entry_premium + pos.call_entry_premium) - (put_close + call_close)
                };
                let position_pnl_dollars = position_pnl * config.simulation.contract_multiplier;
                
                if is_long {
                    pnl_summary.total_premium_collected += put_close + call_close;
                } else {
                    pnl_summary.total_premium_paid += put_close + call_close;
                }
                
                let reason_str = if fractional_dte <= 0.0 { "Expiration" } else { "Roll" };
                let msg = format!("Day {} | CLOSED position {} at {} | P&L: ${:.0} ({})",
                    timestamp.day, pos.position_id.0, &config.strategy.roll_time, position_pnl_dollars, reason_str);
                
                if !json_mode {
                    print!("{} | Price ${:.2} | ", date_str, current_price);
                    println!("{}", msg);
                }
                
                trades.push(TradeEvent {
                    trade_type: "close".to_string(),
                    day: timestamp.day,
                    minute: timestamp.minute,
                    price: current_price,
                    message: msg,
                    pnl: Some(position_pnl_dollars),
                });
                
                // Open new position
                let use_same_strikes = config.strike_config.roll_type == "same_strikes";
                let new_pos = open_position_with_pricing(
                    &calendar, &mut event_store, &mut pnl_summary, &config,
                    timestamp.day, roll_time, current_price,
                    if use_same_strikes { Some((pos.put_strike, pos.call_strike)) } else { None },
                    implied_vol,
                );
                
                let new_total = new_pos.put_entry_premium + new_pos.call_entry_premium;
                let new_total_dollars = new_total * config.simulation.contract_multiplier;
                let new_display = if is_long { -new_total } else { new_total };
                let new_display_dollars = if is_long { -new_total_dollars } else { new_total_dollars };
                
                let msg2 = format!("Day {} | OPENED position {} at {} | Strikes: P${:.2} C${:.2} | ${:.2} (${:.0})",
                    timestamp.day, new_pos.position_id.0, &config.strategy.roll_time,
                    new_pos.put_strike, new_pos.call_strike, new_display, new_display_dollars);
                
                if !json_mode {
                    println!("  -> {}", msg2);
                    print_greeks(&new_pos);
                }
                
                trades.push(TradeEvent {
                    trade_type: "open".to_string(),
                    day: timestamp.day,
                    minute: timestamp.minute,
                    price: current_price,
                    message: msg2,
                    pnl: None,
                });
                
                active_position = Some(new_pos);
                continue;
            } else {
                active_position = Some(pos);
            }
        }

        // Open new position
        if active_position.is_none() && timestamp.minute >= entry_time {
            let pos = open_position_with_pricing(
                &calendar, &mut event_store, &mut pnl_summary, &config,
                timestamp.day, entry_time, current_price, None, implied_vol,
            );

            let is_long = config.strategy.side == "long";
            let total_premium = pos.put_entry_premium + pos.call_entry_premium;
            let total_dollars = total_premium * config.simulation.contract_multiplier;
            let display = if is_long { -total_premium } else { total_premium };
            let display_dollars = if is_long { -total_dollars } else { total_dollars };
            
            let msg = format!("Day {} | OPENED position {} at {} | Strikes: P${:.2} C${:.2} | ${:.2} (${:.0})",
                timestamp.day, pos.position_id.0, &config.strategy.entry_time,
                pos.put_strike, pos.call_strike, display, display_dollars);
            
            if !json_mode {
                print!("{} | Price ${:.2} | ", date_str, current_price);
                println!("{}", msg);
                print_greeks(&pos);
            }
            
            trades.push(TradeEvent {
                trade_type: "open".to_string(),
                day: timestamp.day,
                minute: timestamp.minute,
                price: current_price,
                message: msg,
                pnl: None,
            });

            active_position = Some(pos);
        }
    }

    // Build output
    let net_pnl = pnl_summary.total_premium_collected - pnl_summary.total_premium_paid;
    let final_price = price_points.last().map(|p| p.price).unwrap_or(config.simulation.initial_price);
    
    let output = SimulationOutput {
        config: SimulationConfig {
            days: config.simulation.days,
            resolution_minutes: config.simulation.intraday_resolution_minutes,
            initial_price: config.simulation.initial_price,
            volatility: realized_vol,
            vrp: config.simulation.volatility_risk_premium,
            seed: config.simulation.seed,
        },
        price_points: price_points.iter().map(|p| PricePointJson {
            day: p.timestamp.day,
            minute: p.timestamp.minute,
            price: p.price,
        }).collect(),
        daily_ohlc,
        trades,
        summary: Summary {
            total_positions: pnl_summary.position_count,
            total_premium_collected: pnl_summary.total_premium_collected,
            total_premium_paid: pnl_summary.total_premium_paid,
            net_pnl,
            final_price,
        },
    };

    // Output
    if json_mode {
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("\n{}", "=".repeat(60));
        println!("SIMULATION SUMMARY");
        println!("{}", "=".repeat(60));
        println!("Total positions opened: {}", pnl_summary.position_count);
        println!("Total premium collected: ${:.2} per barrel", pnl_summary.total_premium_collected);
        println!("Total premium paid: ${:.2} per barrel", pnl_summary.total_premium_paid);
        println!("Net P&L: ${:.2} per barrel", net_pnl);
        println!("Net P&L: ${:.0} total", net_pnl * config.simulation.contract_multiplier);
        println!("Final price: ${:.2}", final_price);
    }
}

/// Aggregate 10-min price points to daily OHLC
fn aggregate_to_daily_ohlc(price_points: &[PricePoint]) -> Vec<DailyOHLC> {
    use std::collections::HashMap;
    
    let mut daily_data: HashMap<u32, (f64, f64, f64, f64)> = HashMap::new();
    
    for point in price_points {
        let day = point.timestamp.day;
        let price = point.price;
        
        daily_data.entry(day)
            .and_modify(|(o, h, l, c)| {
                *h = h.max(price);
                *l = l.min(price);
                *c = price;
            })
            .or_insert((price, price, price, price));
    }
    
    let mut result: Vec<DailyOHLC> = daily_data.iter()
        .map(|(&day, &(open, high, low, close))| DailyOHLC { day, open, high, low, close })
        .collect();
    
    result.sort_by_key(|d| d.day);
    result
}

/// Calculate fractional days to expiration
fn calculate_fractional_dte(current: &Timestamp, expiration_day: u32) -> f64 {
    if current.day >= expiration_day {
        return 0.0;
    }
    let days_remaining = (expiration_day - current.day) as f64;
    let minutes_fraction = (138.0 - current.minute as f64) / 138.0;
    days_remaining - 1.0 + minutes_fraction
}

/// Parse time string
fn parse_time(time_str: &str) -> u32 {
    let parts: Vec<&str> = time_str.split(':').collect();
    let hours: u32 = parts[0].parse().unwrap_or(14);
    let minutes: u32 = parts[1].parse().unwrap_or(0);
    hours * 60 + minutes
}

/// Format timestamp
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

/// Calculate intrinsic value
fn calculate_intrinsic(underlying: f64, strike: f64, is_call: bool) -> f64 {
    if is_call {
        (underlying - strike).max(0.0)
    } else {
        (strike - underlying).max(0.0)
    }
}

/// Open position
fn open_position_with_pricing(
    _calendar: &TradingCalendar,
    event_store: &mut EventStore,
    pnl: &mut PnLSummary,
    config: &Config,
    entry_day: u32,
    entry_time: u32,
    current_price: f64,
    strike_override: Option<(f64, f64)>,
    implied_vol: f64,
) -> PositionTracking {
    let calendar_old = calendar::Calendar::new();
    let mut expiration_day = entry_day;
    let mut trading_days_count = 0;
    while trading_days_count < config.strategy.entry_dte {
        expiration_day = calendar_old.next_trading_day(expiration_day);
        trading_days_count += 1;
    }
    let time_to_expiry = config.strategy.entry_dte as f64 / 252.0;

    let position_id = event_store.next_position_id();
    let put_leg_id = event_store.next_leg_id();
    let call_leg_id = event_store.next_leg_id();

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

    let put_premium = Black76::price(
        current_price, put_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, false
    );
    let call_premium = Black76::price(
        current_price, call_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, true
    );

    let put_greeks = Black76::greeks(
        current_price, put_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, false
    );
    let call_greeks = Black76::greeks(
        current_price, call_strike, time_to_expiry,
        config.simulation.risk_free_rate, implied_vol, true
    );

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

/// Print Greeks
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
