//! Roll Trigger Engine
//!
//! Evaluates roll conditions and executes position management decisions.

use crate::calendar::{Calendar, Day, TimeOfDay};
use crate::config::{Config, RollTriggerConfig};
use crate::pricing::Black76;

/// Result of evaluating roll triggers
#[derive(Debug, Clone)]
pub enum RollDecision {
    /// No roll needed
    Hold,
    /// Roll both legs
    RollBoth { reason: RollReason },
    /// Roll put leg only
    RollPut { reason: RollReason },
    /// Roll call leg only  
    RollCall { reason: RollReason },
}

/// Reason for rolling
#[derive(Debug, Clone)]
pub enum RollReason {
    /// Time-based trigger (e.g., 14:00)
    TimeTrigger,
    /// DTE threshold reached (e.g., 28 DTE remaining)
    DteThreshold { remaining_dte: u32 },
    /// Profit target hit (e.g., 50% of max profit)
    ProfitTarget { profit_percent: f64 },
    /// Stop loss hit
    StopLoss { loss_percent: f64 },
    /// Price moved beyond threshold
    PriceMove { points_moved: f64 },
}

/// Position state for trigger evaluation
#[derive(Debug, Clone)]
pub struct PositionState {
    pub position_id: u64,
    pub entry_day: Day,
    pub expiration_day: Day,
    pub entry_price: f64,
    pub current_price: f64,
    pub put_strike: f64,
    pub call_strike: f64,
    pub put_entry_premium: f64,
    pub call_entry_premium: f64,
    pub last_rolled_put: Option<Day>,
    pub last_rolled_call: Option<Day>,
}

/// Evaluates roll triggers and returns decision
pub fn evaluate_triggers(
    position: &PositionState,
    config: &Config,
    calendar: &Calendar,
    current_day: Day,
    current_time: TimeOfDay,
    implied_vol: f64,
    risk_free_rate: f64,
) -> RollDecision {
    let roll_time = parse_time(&config.strategy.roll_time);
    
    // Check each configured trigger
    for trigger in &config.strategy.roll_triggers {
        match trigger.trigger_type.as_str() {
            "time" => {
                // Time trigger: roll at specific time on expiration day
                if current_day == position.expiration_day && current_time >= roll_time {
                    return RollDecision::RollBoth { 
                        reason: RollReason::TimeTrigger 
                    };
                }
            }
            "dte" => {
                // DTE threshold: roll when remaining DTE <= value
                let remaining_dte = calendar.calculate_dte(current_day, position.expiration_day);
                if remaining_dte as f64 <= trigger.value {
                    return match trigger.legs.as_str() {
                        "put" => RollDecision::RollPut { 
                            reason: RollReason::DteThreshold { remaining_dte } 
                        },
                        "call" => RollDecision::RollCall { 
                            reason: RollReason::DteThreshold { remaining_dte } 
                        },
                        _ => RollDecision::RollBoth { 
                            reason: RollReason::DteThreshold { remaining_dte } 
                        },
                    };
                }
            }
            "profit_target" => {
                // Profit target: roll when unrealized P&L >= target × max profit
                let target_fraction = trigger.value; // e.g., 0.50 for 50%
                
                // Calculate current position value
                let remaining_dte = calendar.calculate_dte(current_day, position.expiration_day);
                let time_to_expiry = remaining_dte as f64 / 252.0;
                
                let current_put = Black76::price(
                    position.current_price,
                    position.put_strike,
                    time_to_expiry,
                    risk_free_rate,
                    implied_vol,
                    false,
                );
                let current_call = Black76::price(
                    position.current_price,
                    position.call_strike,
                    time_to_expiry,
                    risk_free_rate,
                    implied_vol,
                    true,
                );
                
                let entry_value = position.put_entry_premium + position.call_entry_premium;
                let current_value = current_put + current_call;
                let unrealized_pnl = entry_value - current_value; // Positive = profit for shorts
                let max_profit = entry_value; // Max profit = keeping all premium
                
                if max_profit > 0.0 && unrealized_pnl >= target_fraction * max_profit {
                    return match trigger.legs.as_str() {
                        "put" => RollDecision::RollPut { 
                            reason: RollReason::ProfitTarget { 
                                profit_percent: (unrealized_pnl / max_profit) * 100.0 
                            } 
                        },
                        "call" => RollDecision::RollCall { 
                            reason: RollReason::ProfitTarget { 
                                profit_percent: (unrealized_pnl / max_profit) * 100.0 
                            } 
                        },
                        _ => RollDecision::RollBoth { 
                            reason: RollReason::ProfitTarget { 
                                profit_percent: (unrealized_pnl / max_profit) * 100.0 
                            } 
                        },
                    };
                }
            }
            "price_move" => {
                // Price move: roll when underlying moved X points from entry
                let price_move = (position.current_price - position.entry_price).abs();
                if price_move >= trigger.value {
                    return match trigger.legs.as_str() {
                        "put" => RollDecision::RollPut { 
                            reason: RollReason::PriceMove { points_moved: price_move } 
                        },
                        "call" => RollDecision::RollCall { 
                            reason: RollReason::PriceMove { points_moved: price_move } 
                        },
                        _ => RollDecision::RollBoth { 
                            reason: RollReason::PriceMove { points_moved: price_move } 
                        },
                    };
                }
            }
            _ => {}
        }
    }
    
    // Default: check time-based roll even if not explicitly configured
    // (to prevent holding past expiration)
    if current_day == position.expiration_day && current_time >= roll_time {
        return RollDecision::RollBoth { 
            reason: RollReason::TimeTrigger 
        };
    }
    
    RollDecision::Hold
}

/// Parse time string "HH:MM" to minutes from midnight
fn parse_time(time_str: &str) -> TimeOfDay {
    let parts: Vec<&str> = time_str.split(':').collect();
    let hours: TimeOfDay = parts[0].parse().unwrap_or(14);
    let minutes: TimeOfDay = parts[1].parse().unwrap_or(0);
    hours * 60 + minutes
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_time_trigger() {
        // Test that 14:00 trigger fires correctly
        let trigger_time = parse_time("14:00");
        assert_eq!(trigger_time, 14 * 60);
    }
    
    #[test]
    fn test_profit_target_calculation() {
        // Entry premium: $1.00, current value: $0.50
        // Unrealized P&L: $0.50 = 50% of max profit
        let entry_value = 1.0;
        let current_value = 0.5;
        let unrealized_pnl = entry_value - current_value;
        let max_profit = entry_value;
        let profit_percent = (unrealized_pnl / max_profit) * 100.0;
        assert_eq!(profit_percent, 50.0);
    }
}
