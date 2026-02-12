//! Trading Simulator V2 - Phase 1: Hardcoded 1DTE Straddle
//! 
//! This is a proof-of-concept implementation demonstrating:
//! - Synthetic calendar
//! - Event sourcing
//! - Basic 1DTE straddle strategy

mod calendar;
mod events;

use calendar::{Calendar, Day, TimeOfDay};
use events::{Event, EventStore, OptionContract, OptionType, PositionId, Side, LegId, RollTrigger};

/// Simple 1DTE straddle strategy configuration
#[derive(Debug)]
struct StraddleConfig {
    /// How many points OTM for each leg (0.0 = ATM)
    strike_offset: f64,
    /// Time to roll (14:00 = 840 minutes from midnight)
    roll_time: TimeOfDay,
    /// Don't roll if we already rolled today
    per_leg_cooldown: bool,
}

impl Default for StraddleConfig {
    fn default() -> Self {
        Self {
            strike_offset: 0.0,  // ATM
            roll_time: 14 * 60,  // 14:00
            per_leg_cooldown: true,
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
    /// Day this position expires
    expiration_day: Day,
    /// Last day we rolled each leg (for cooldown)
    put_last_rolled: Option<Day>,
    call_last_rolled: Option<Day>,
    /// Entry price of underlying
    entry_price: f64,
}

fn main() {
    println!("Trading Simulator V2 - Phase 1: 1DTE Straddle\n");
    
    let calendar = Calendar::new();
    let mut event_store = EventStore::new();
    let config = StraddleConfig::default();
    
    // Simulation parameters
    let start_day: Day = 0;  // Monday, Jan 1, Year 0
    let num_days = 20;       // Run for 20 days
    let initial_price = 75.0; // /CL around $75
    
    // Track active position
    let mut active_position: Option<StraddleState> = None;
    
    // Simple price simulation (random walk)
    let mut current_price = initial_price;
    
    println!("Simulation starting:");
    println!("  Start day: {} (Monday)", start_day);
    println!("  Initial price: ${:.2}", initial_price);
    println!("  Roll time: 14:00");
    println!();
    
    // Run simulation day by day
    for day in start_day..start_day + num_days {
        if !calendar.is_trading_day(day) {
            continue;
        }
        
        // Simple price drift (in real code, this comes from price generator)
        current_price += (day as f64 * 0.1).sin() * 0.5;
        
        let date_str = format_day(day);
        print!("Day {} ({}): Price ${:.2} | ", day, date_str, current_price);
        
        // Check if we need to open a new position
        if active_position.is_none() {
            // Open new 1DTE straddle
            let expiration = calendar.next_trading_day(day);
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
                expiration_day: expiration,
            };
            
            let call_contract = OptionContract {
                underlying_price: current_price,
                strike: call_strike,
                option_type: OptionType::Call,
                side: Side::Short,
                expiration_day: expiration,
            };
            
            // Simulate premiums (in real code, from Black-Scholes)
            let put_premium = 0.50;
            let call_premium = 0.50;
            
            let event = Event::PositionOpened {
                position_id,
                timestamp: (day, 9 * 60), // 9:00 AM entry
                legs: vec![
                    (put_leg_id, put_contract, put_premium),
                    (call_leg_id, call_contract, call_premium),
                ],
            };
            
            event_store.append(event);
            
            active_position = Some(StraddleState {
                position_id,
                put_leg: put_leg_id,
                call_leg: call_leg_id,
                entry_day: day,
                expiration_day: expiration,
                put_last_rolled: None,
                call_last_rolled: None,
                entry_price: current_price,
            });
            
            println!("OPENED 1DTE straddle (Put ${:.2}, Call ${:.2}) -> Exp {}", 
                     put_strike, call_strike, expiration);
        } else {
            let pos = active_position.as_ref().unwrap();
            let remaining_dte = calendar.calculate_dte(day, pos.expiration_day);
            
            // Check for roll trigger (14:00 on expiration day)
            if day == pos.expiration_day && remaining_dte == 0 {
                println!("ROLLING at 14:00 (expiration day)");
                
                // In full implementation, this would:
                // 1. Close current position at 14:30 expiration
                // 2. Open new 1DTE for tomorrow
                
                // For now, just close and mark for reopening
                let close_event = Event::PositionClosed {
                    position_id: pos.position_id,
                    timestamp: (day, config.roll_time),
                    close_premiums: vec![
                        (pos.put_leg, 0.10),  // Simulated decayed value
                        (pos.call_leg, 0.10),
                    ],
                    reason: events::CloseReason::Expiration,
                };
                event_store.append(close_event);
                
                active_position = None; // Will open new position tomorrow
            } else {
                println!("Holding (DTE: {})", remaining_dte);
            }
        }
    }
    
    // Summary
    println!("\n=== Simulation Summary ===");
    println!("Total events recorded: {}", event_store.all_events().len());
    
    // Replay events for one position
    for event in event_store.all_events() {
        match event {
            Event::PositionOpened { position_id, timestamp, legs } => {
                println!("\nPosition {} opened on day {} at {}:{:02}",
                    position_id.0, timestamp.0, 
                    timestamp.1 / 60, timestamp.1 % 60);
                for (leg_id, contract, premium) in legs {
                    println!("  Leg {}: {:?} {:?} strike=${:.2} premium=${:.2}",
                        leg_id.0, contract.side, contract.option_type,
                        contract.strike, premium);
                }
            }
            Event::PositionClosed { position_id, timestamp, close_premiums, reason } => {
                println!("Position {} closed on day {} at {}:{:02} (reason: {:?})",
                    position_id.0, timestamp.0,
                    timestamp.1 / 60, timestamp.1 % 60, reason);
            }
            _ => {}
        }
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
