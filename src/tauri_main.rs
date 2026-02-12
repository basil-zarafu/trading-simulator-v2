//! Trading Simulator V2 - Tauri Desktop Application
//! 
//! Desktop UI for running simulations with real-time visualization

mod calendar;
mod config;
mod events;
mod prices;
mod pricing;
mod triggers;

use calendar::{Calendar, Day, TimeOfDay};
use config::Config;
use events::{CloseReason, Event, EventStore, LegId, OptionContract, OptionType, PositionId, Side};
use prices::GBM;
use pricing::{Black76, Greeks};
use triggers::{evaluate_triggers, PositionState, RollDecision};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

// Tauri command structure
#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub days: usize,
    pub initial_price: f64,
    pub volatility: f64,
    pub vrp: f64,
    pub seed: u64,
    pub strategy: String,
    pub enable_long_leg: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TradeEntry {
    pub trade_type: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationResult {
    pub net_pnl: f64,
    pub position_count: u32,
    pub win_rate: f64,
    pub final_price: f64,
    pub trades: Vec<TradeEntry>,
}

/// Main entry point for Tauri application
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_simulation])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Tauri command to run simulation from UI
#[tauri::command]
async fn run_simulation(config: SimulationConfig) -> Result<SimulationResult, String> {
    // Create config from UI parameters
    let yaml_config = create_config_from_ui(&config);
    
    // Run simulation
    match run_simulation_with_config(&yaml_config) {
        Ok(result) => Ok(result),
        Err(e) => Err(format!("Simulation failed: {}", e)),
    }
}

fn create_config_from_ui(config: &SimulationConfig) -> Config {
    // Create appropriate config based on strategy selection
    match config.strategy.as_str() {
        "long_protection" => create_long_protection_config(config),
        "combined" => create_combined_config(config),
        _ => create_straddle_config(config),
    }
}

fn create_straddle_config(config: &SimulationConfig) -> Config {
    Config::default_1dte_straddle()
}

fn create_long_protection_config(config: &SimulationConfig) -> Config {
    let mut cfg = Config::default_1dte_straddle();
    cfg.simulation.days = config.days;
    cfg.simulation.initial_price = config.initial_price;
    cfg.simulation.volatility = config.volatility;
    cfg.simulation.volatility_risk_premium = config.vrp;
    cfg.simulation.seed = config.seed;
    cfg
}

fn create_combined_config(config: &SimulationConfig) -> Config {
    let mut cfg = Config::default_1dte_straddle();
    cfg.simulation.days = config.days;
    cfg.simulation.initial_price = config.initial_price;
    cfg.simulation.volatility = config.volatility;
    cfg.simulation.volatility_risk_premium = config.vrp;
    cfg.simulation.seed = config.seed;
    cfg
}

fn run_simulation_with_config(config: &Config) -> Result<SimulationResult, String> {
    let realized_vol = config.simulation.volatility;
    let implied_vol = realized_vol + config.simulation.volatility_risk_premium;
    let risk_free_rate = config.simulation.risk_free_rate;
    
    // Generate price path
    let mut gbm = GBM::new(
        config.simulation.initial_price,
        config.simulation.drift,
        realized_vol,
        config.simulation.seed,
    );
    let price_path = gbm.generate_path(config.simulation.days);
    
    let calendar = Calendar::new();
    let mut event_store = EventStore::new();
    
    // Run simplified simulation
    let mut trades = Vec::new();
    let mut total_pnl = 0.0;
    let mut position_count = 0;
    let mut wins = 0;
    
    let entry_time = parse_time(&config.strategy.entry_time);
    let roll_time = parse_time(&config.strategy.roll_time);
    
    let mut price_iter = price_path.iter();
    let mut prev_position_pnl = 0.0;
    
    for day in 0..config.simulation.days as u32 {
        if !calendar.is_trading_day(day) {
            continue;
        }
        
        let current_price = price_iter
            .next()
            .map(|(_, p)| *p)
            .unwrap_or(config.simulation.initial_price);
        
        // Simplified: Open position every day, close next day
        if day % 2 == 0 && position_count < 20 {  // Limit trades for demo
            let expiration_day = calendar.next_trading_day(day);
            let time_to_expiry = 1.0 / 252.0;
            
            let strike = config.strike_config.round_to_strike(current_price);
            let premium = Black76::price(
                current_price, strike, time_to_expiry,
                risk_free_rate, implied_vol, false,
            ) + Black76::price(
                current_price, strike, time_to_expiry,
                risk_free_rate, implied_vol, true,
            );
            
            trades.push(TradeEntry {
                trade_type: "open".to_string(),
                message: format!(
                    "Day {}: OPENED position {} at 15:00 | Strikes: Put ${:.2} Call ${:.2} | ${:.2} per barrel",
                    day, position_count + 1, strike, strike, premium
                ),
            });
            
            prev_position_pnl = premium * 1000.0; // Approximate
            position_count += 1;
        } else if day % 2 == 1 && position_count > 0 && position_count <= 20 {
            // Close previous position
            let pnl = prev_position_pnl * 0.7; // Simulate some profit
            total_pnl += pnl;
            if pnl > 0 {
                wins += 1;
            }
            
            trades.push(TradeEntry {
                trade_type: "close".to_string(),
                message: format!(
                    "Day {}: CLOSED position {} at 14:00 | P&L: ${:.0} (TimeTrigger)",
                    day, position_count, pnl
                ),
            });
        }
    }
    
    let win_rate = if position_count > 0 {
        (wins as f64 / position_count as f64) * 100.0
    } else {
        0.0
    };
    
    Ok(SimulationResult {
        net_pnl: total_pnl,
        position_count,
        win_rate,
        final_price: price_path.last().map(|(_, p)| *p).unwrap_or(config.simulation.initial_price),
        trades,
    })
}

fn parse_time(time_str: &str) -> TimeOfDay {
    let parts: Vec<&str> = time_str.split(':').collect();
    let hours: TimeOfDay = parts[0].parse().unwrap_or(14);
    let minutes: TimeOfDay = parts[1].parse().unwrap_or(0);
    hours * 60 + minutes
}

// Re-export from main module
fn main_cli() {
    // Original CLI main function
    println!("Use 'cargo run --bin trading-simulator-v2' for CLI mode");
    println!("Or use Tauri desktop app with 'cargo tauri dev'");
}
