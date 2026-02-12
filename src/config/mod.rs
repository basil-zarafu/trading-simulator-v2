//! YAML Configuration for Trading Simulator V2
//!
//! This module handles loading strategy and simulation parameters from YAML files.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Simulation settings
    pub simulation: SimulationConfig,
    /// Strategy configuration
    pub strategy: StrategyConfig,
    /// Product-specific settings (optional overrides)
    #[serde(default)]
    pub product: Option<ProductConfig>,
    /// Strike configuration
    #[serde(default = "default_strike_config")]
    pub strike_config: StrikeConfig,
}

/// Simulation parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    /// Number of days to simulate
    pub days: usize,
    /// Initial underlying price
    pub initial_price: f64,
    /// Annual drift (μ), e.g., 0.0 for no drift
    #[serde(default)]
    pub drift: f64,
    /// Annual volatility (σ), e.g., 0.30 for 30%
    pub volatility: f64,
    /// Random seed for reproducibility
    pub seed: u64,
    /// Risk-free rate (e.g., 0.05 for 5%)
    #[serde(default = "default_risk_free_rate")]
    pub risk_free_rate: f64,
    /// Contract multiplier (1000 for /CL, 100 for stocks)
    #[serde(default = "default_contract_multiplier")]
    pub contract_multiplier: f64,
}

/// Strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Strategy type: "straddle", "strangle", etc.
    pub strategy_type: String,
    /// DTE (days to expiration) at entry
    pub entry_dte: u32,
    /// Entry time in HH:MM format
    #[serde(default = "default_entry_time")]
    pub entry_time: String,
    /// Roll time in HH:MM format
    #[serde(default = "default_roll_time")]
    pub roll_time: String,
    /// Strike selection: "ATM", "OTM", or specific offset
    #[serde(default = "default_strike_selection")]
    pub strike_selection: String,
    /// Strike offset in price points (for OTM strategies)
    #[serde(default)]
    pub strike_offset: f64,
    /// Roll triggers
    #[serde(default)]
    pub roll_triggers: Vec<RollTriggerConfig>,
}

/// Roll trigger configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollTriggerConfig {
    /// Trigger type: "time", "dte", "profit_target", "stop_loss"
    pub trigger_type: String,
    /// Value for the trigger (interpretation depends on type)
    pub value: f64,
    /// Optional: which legs this applies to ("both", "put", "call")
    #[serde(default = "default_legs")]
    pub legs: String,
}

/// Product-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductConfig {
    /// Product symbol (e.g., "/CL", "/ES", "SPX")
    pub symbol: String,
    /// Tick size (minimum price increment)
    pub tick_size: f64,
    /// Point value in dollars
    pub point_value: f64,
    /// Trading hours
    pub trading_hours: TradingHoursConfig,
}

/// Trading hours configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingHoursConfig {
    /// Market open time in HH:MM
    pub open: String,
    /// Market close time in HH:MM
    pub close: String,
    /// Option expiration time in HH:MM
    pub option_expiry: String,
}

/// Strike configuration for a product
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrikeConfig {
    /// Strike tick size (0.25 for /CL, 1.0 for SPY, 5.0 for SPX)
    #[serde(default = "default_strike_tick_size")]
    pub tick_size: f64,
    /// Roll type: "recenter" (to ATM) or "same_strikes" (keep old strikes)
    #[serde(default = "default_roll_type")]
    pub roll_type: String,
}

impl StrikeConfig {
    /// Round a price to the nearest valid strike
    pub fn round_to_strike(&self, price: f64) -> f64 {
        (price / self.tick_size).round() * self.tick_size
    }
    
    /// Round down to nearest strike (for puts when going OTM)
    pub fn round_down_to_strike(&self, price: f64) -> f64 {
        (price / self.tick_size).floor() * self.tick_size
    }
    
    /// Round up to nearest strike (for calls when going OTM)
    pub fn round_up_to_strike(&self, price: f64) -> f64 {
        (price / self.tick_size).ceil() * self.tick_size
    }
    
    /// Get strikes for a straddle given underlying price
    pub fn get_straddle_strikes(&self, underlying: f64, offset: f64) -> (f64, f64) {
        let atm = self.round_to_strike(underlying);
        let put_strike = self.round_to_strike(atm - offset);
        let call_strike = self.round_to_strike(atm + offset);
        (put_strike, call_strike)
    }
    
    /// Find nearest available strike
    pub fn nearest_strike(&self, price: f64) -> f64 {
        self.round_to_strike(price)
    }
}

impl Config {
    /// Load configuration from a YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    /// Create a default configuration (1DTE straddle)
    pub fn default_1dte_straddle() -> Self {
        Self {
            simulation: SimulationConfig {
                days: 30,
                initial_price: 75.0,
                drift: 0.0,
                volatility: 0.30,
                seed: 42,
                risk_free_rate: 0.05,
                contract_multiplier: 1000.0,
            },
            strategy: StrategyConfig {
                strategy_type: "straddle".to_string(),
                entry_dte: 1,
                entry_time: "15:00".to_string(),
                roll_time: "14:00".to_string(),
                strike_selection: "ATM".to_string(),
                strike_offset: 0.0,
                roll_triggers: vec![
                    RollTriggerConfig {
                        trigger_type: "time".to_string(),
                        value: 14.0, // 14:00
                        legs: "both".to_string(),
                    },
                ],
            },
            product: Some(ProductConfig {
                symbol: "/CL".to_string(),
                tick_size: 0.01,
                point_value: 1000.0,
                trading_hours: TradingHoursConfig {
                    open: "09:00".to_string(),
                    close: "17:00".to_string(),
                    option_expiry: "14:30".to_string(),
                },
            }),
            strike_config: StrikeConfig {
                tick_size: 0.25,
                roll_type: "recenter".to_string(),
            },
        }
    }

    /// Validate the configuration
    fn validate(&self) -> Result<(), ConfigError> {
        // Check volatility is positive
        if self.simulation.volatility <= 0.0 {
            return Err(ConfigError::Validation(
                "Volatility must be positive".to_string()
            ));
        }

        // Check days is reasonable
        if self.simulation.days == 0 || self.simulation.days > 10000 {
            return Err(ConfigError::Validation(
                "Simulation days must be between 1 and 10000".to_string()
            ));
        }

        // Validate strategy type
        let valid_strategies = ["straddle", "strangle", "iron_condor"];
        if !valid_strategies.contains(&self.strategy.strategy_type.as_str()) {
            return Err(ConfigError::Validation(
                format!("Unknown strategy type: {}", self.strategy.strategy_type)
            ));
        }

        Ok(())
    }

    /// Save configuration to a YAML file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let yaml = serde_yaml::to_string(self)?;
        fs::write(path, yaml)?;
        Ok(())
    }
}

/// Configuration errors
#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(serde_yaml::Error),
    Validation(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO error: {}", e),
            ConfigError::Parse(e) => write!(f, "Parse error: {}", e),
            ConfigError::Validation(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io(e) => Some(e),
            ConfigError::Parse(e) => Some(e),
            ConfigError::Validation(_) => None,
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(e: serde_yaml::Error) -> Self {
        ConfigError::Parse(e)
    }
}

// Default value functions
fn default_risk_free_rate() -> f64 {
    0.05
}

fn default_contract_multiplier() -> f64 {
    1000.0
}

fn default_entry_time() -> String {
    "15:00".to_string()
}

fn default_roll_time() -> String {
    "14:00".to_string()
}

fn default_strike_selection() -> String {
    "ATM".to_string()
}

fn default_legs() -> String {
    "both".to_string()
}

fn default_strike_config() -> StrikeConfig {
    StrikeConfig {
        tick_size: 0.25,
        roll_type: "recenter".to_string(),
    }
}

fn default_strike_tick_size() -> f64 {
    0.25
}

fn default_roll_type() -> String {
    "recenter".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default_1dte_straddle();
        assert_eq!(config.strategy.entry_dte, 1);
        assert_eq!(config.simulation.contract_multiplier, 1000.0);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default_1dte_straddle();
        config.simulation.volatility = -0.1;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_yaml_roundtrip() {
        let config = Config::default_1dte_straddle();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.strategy.entry_dte, config.strategy.entry_dte);
    }
}
