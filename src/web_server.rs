//! Web server for Trading Simulator UI
//! 
//! Serves static files and provides API for running simulations

use actix_web::{web, App, HttpResponse, HttpServer, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct SimRequest {
    days: usize,
    initial_price: f64,
    volatility: f64,
    vrp: f64,
    seed: u64,
    strategy: String,
}

#[derive(Debug, Serialize)]
struct SimResponse {
    net_pnl: f64,
    position_count: u32,
    win_rate: f64,
    final_price: f64,
    trades: Vec<TradeEntry>,
}

#[derive(Debug, Serialize)]
struct TradeEntry {
    trade_type: String,
    message: String,
}

async fn run_simulation(req: web::Json<SimRequest>) -> Result<HttpResponse> {
    // Create temporary config file
    let config_yaml = format!(r#"
simulation:
  days: {}
  initial_price: {:.2}
  drift: 0.0
  volatility: {:.2}
  volatility_risk_premium: {:.2}
  seed: {}
  risk_free_rate: 0.05
  contract_multiplier: 1000

strategy:
  strategy_type: straddle
  entry_dte: 1
  entry_time: "15:00"
  roll_time: "14:00"
  strike_selection: ATM
  side: "short"
  roll_triggers:
    - trigger_type: time
      value: 14.0
      legs: both

strike_config:
  tick_size: 0.25
  roll_type: recenter
"#, req.days, req.initial_price, req.volatility, req.vrp, req.seed);

    let config_path = "/tmp/sim_config.yaml";
    std::fs::write(config_path, config_yaml).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to write config: {}", e))
    })?;

    // Run simulation using pre-built binary
    let output = Command::new("/home/gutastef/trading-simulator-v2/target/debug/trading-simulator-v2")
        .arg(config_path)
        .output()
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to run simulation: {}", e))
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Parse output to extract trades and P&L
    let trades = parse_simulation_output(&stdout);
    let (net_pnl, position_count, final_price) = extract_summary(&stdout);
    
    let win_rate = if position_count > 0 {
        let wins = trades.iter().filter(|t| t.trade_type == "close" && !t.message.contains("-$")).count();
        (wins as f64 / position_count as f64) * 100.0
    } else {
        0.0
    };

    Ok(HttpResponse::Ok().json(SimResponse {
        net_pnl,
        position_count,
        win_rate,
        final_price,
        trades,
    }))
}

fn parse_simulation_output(output: &str) -> Vec<TradeEntry> {
    let mut trades = Vec::new();
    
    for line in output.lines() {
        if line.contains("OPENED position") {
            trades.push(TradeEntry {
                trade_type: "open".to_string(),
                message: line.trim().to_string(),
            });
        } else if line.contains("CLOSED position") {
            trades.push(TradeEntry {
                trade_type: "close".to_string(),
                message: line.trim().to_string(),
            });
        } else if line.contains("Holding pos") {
            trades.push(TradeEntry {
                trade_type: "hold".to_string(),
                message: line.trim().to_string(),
            });
        }
    }
    
    // Limit to first 50 entries for UI performance
    trades.truncate(50);
    trades
}

fn extract_summary(output: &str) -> (f64, u32, f64) {
    let mut net_pnl = 0.0;
    let mut position_count = 0;
    let mut final_price = 0.0;
    
    for line in output.lines() {
        if line.contains("Net P&L:") {
            // Extract total P&L value (in parentheses)
            // Format: "Net P&L: $13.44 per barrel ($13441 total)"
            if let Some(start) = line.find("($") {
                if let Some(end) = line[start..].find(" total") {
                    let val = &line[start+2..start+end];
                    net_pnl = val.replace(",", "").parse().unwrap_or(0.0);
                }
            }
        }
        if line.contains("Total positions opened:") {
            position_count = line.split(':').nth(1)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
        }
        if line.contains("Final underlying price:") {
            final_price = line.split('$').nth(1)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0.0);
        }
    }
    
    (net_pnl, position_count, final_price)
}

async fn index() -> Result<HttpResponse> {
    // Serve the index.html file
    let html = include_str!("../ui/index.html");
    Ok(HttpResponse::Ok()
        .content_type("text/html")
        .body(html))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("🚀 Trading Simulator Web Server starting...");
    println!("📱 Open http://localhost:3000 in your browser");
    println!("");
    
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(index))
            .route("/run", web::post().to(run_simulation))
    })
    .bind("127.0.0.1:3000")?
    .run()
    .await
}
