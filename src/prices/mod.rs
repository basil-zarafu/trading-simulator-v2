//! Price Generation Models
//!
//! Geometric Brownian Motion for simulating underlying price paths.
//! Supports both daily and intraday (10-minute) resolution.

use crate::calendar::intraday::{TradingCalendar, Timestamp};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

/// Price point at a specific timestamp
#[derive(Debug, Clone, Copy)]
pub struct PricePoint {
    /// Timestamp (day and minute)
    pub timestamp: Timestamp,
    /// Price at this timestamp
    pub price: f64,
}

/// Geometric Brownian Motion price generator
#[derive(Debug, Clone)]
pub struct GBM {
    /// Initial price (S₀)
    initial_price: f64,
    /// Annual drift (μ)
    drift: f64,
    /// Annual volatility (σ)
    volatility: f64,
    /// Random number generator
    rng: StdRng,
}

impl GBM {
    /// Create a new GBM generator
    ///
    /// # Arguments
    /// * `initial_price` - Starting price (e.g., 75.0 for /CL at $75)
    /// * `drift` - Annual expected return (e.g., 0.05 for 5%)
    /// * `volatility` - Annual volatility (e.g., 0.30 for 30%)
    /// * `seed` - Random seed for reproducibility
    pub fn new(initial_price: f64, drift: f64, volatility: f64, seed: u64) -> Self {
        Self {
            initial_price,
            drift,
            volatility,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Generate a price path for N trading days (legacy daily mode)
    ///
    /// Returns a Vec of (day, price) tuples
    pub fn generate_path(&mut self, num_days: usize) -> Vec<(u32, f64)> {
        let dt: f64 = 1.0 / 252.0; // One trading day in years
        let mut prices = Vec::with_capacity(num_days);
        let mut current_price = self.initial_price;

        for day in 0..num_days {
            prices.push((day as u32, current_price));
            
            // GBM formula: dS = μS dt + σS dW
            let z: f64 = self.rng.sample(rand_distr::StandardNormal);
            let brownian_motion = z * dt.sqrt();
            
            let drift_term = (self.drift - 0.5 * self.volatility.powi(2)) * dt;
            let diffusion_term = self.volatility * brownian_motion;
            
            current_price *= (drift_term + diffusion_term).exp();
        }

        prices
    }

    /// Generate intraday price path with single price points
    ///
    /// # Arguments
    /// * `calendar` - Trading calendar for valid trading times
    /// * `num_days` - Number of trading days to simulate
    /// * `interval_minutes` - Interval between price points (typically 10 for /CL)
    /// * `start_day` - Starting day number
    /// * `start_minute` - Starting minute of day
    ///
    /// # Returns
    /// Vector of PricePoint for each interval
    pub fn generate_intraday_path(
        &mut self,
        calendar: &TradingCalendar,
        num_days: usize,
        interval_minutes: u32,
        start_day: u32,
        start_minute: u32,
    ) -> Vec<PricePoint> {
        // Calculate total number of points needed
        // For 23/5 trading: ~138 points per day at 10-min intervals
        let points_per_day = (23 * 60) as usize / interval_minutes as usize;
        let total_points = num_days * points_per_day;
        
        // Generate trading timestamps
        let timestamps = calendar.generate_trading_times(start_day, start_minute, total_points, interval_minutes);
        
        // Calculate dt per interval in years
        let dt_years = interval_minutes as f64 / (365.25 * 24.0 * 60.0);
        
        let mut points = Vec::with_capacity(timestamps.len());
        let mut current_price = self.initial_price;
        
        for timestamp in timestamps {
            // Generate next price using GBM
            let z: f64 = self.rng.sample(rand_distr::StandardNormal);
            let brownian_motion = z * dt_years.sqrt();
            
            let drift_term = (self.drift - 0.5 * self.volatility.powi(2)) * dt_years;
            let diffusion_term = self.volatility * brownian_motion;
            
            current_price *= (drift_term + diffusion_term).exp();
            
            points.push(PricePoint {
                timestamp,
                price: current_price,
            });
        }
        
        points
    }

    /// Generate a single next price given current price
    ///
    /// Useful for step-by-step simulation
    pub fn next_price(&mut self, current_price: f64) -> f64 {
        let dt: f64 = 1.0 / 252.0;
        let z: f64 = self.rng.sample(rand_distr::StandardNormal);
        let brownian_motion = z * dt.sqrt();
        
        let drift_term = (self.drift - 0.5 * self.volatility.powi(2)) * dt;
        let diffusion_term = self.volatility * brownian_motion;
        
        current_price * (drift_term + diffusion_term).exp()
    }

    /// Reset with a new seed
    pub fn reseed(&mut self, seed: u64) {
        self.rng = StdRng::seed_from_u64(seed);
    }
}

/// Simple deterministic price generator for testing
///
/// Generates a sine wave around a base price
pub struct DeterministicPrice {
    base_price: f64,
    amplitude: f64,
    frequency: f64,
}

impl DeterministicPrice {
    pub fn new(base_price: f64, amplitude: f64, frequency: f64) -> Self {
        Self {
            base_price,
            amplitude,
            frequency,
        }
    }

    pub fn price_at(&self, day: u32) -> f64 {
        self.base_price + (day as f64 * self.frequency).sin() * self.amplitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gbm_reproducibility() {
        // Same seed should produce same path
        let mut gbm1 = GBM::new(75.0, 0.05, 0.30, 42);
        let mut gbm2 = GBM::new(75.0, 0.05, 0.30, 42);
        
        let path1 = gbm1.generate_path(10);
        let path2 = gbm2.generate_path(10);
        
        assert_eq!(path1.len(), path2.len());
        for i in 0..path1.len() {
            assert!((path1[i].1 - path2[i].1).abs() < 1e-10);
        }
    }

    #[test]
    fn test_gbm_starts_at_initial() {
        let mut gbm = GBM::new(75.0, 0.05, 0.30, 123);
        let path = gbm.generate_path(5);
        
        assert_eq!(path[0].1, 75.0);
    }

    #[test]
    fn test_deterministic_price() {
        let price_gen = DeterministicPrice::new(75.0, 0.5, 0.1);
        
        // Same day should always give same price
        let p1 = price_gen.price_at(5);
        let p2 = price_gen.price_at(5);
        assert_eq!(p1, p2);
    }
}
