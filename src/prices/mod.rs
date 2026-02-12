//! Price Generation Models
//!
//! Geometric Brownian Motion for simulating underlying price paths.

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

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

    /// Generate a price path for N trading days
    ///
    /// Returns a Vec of (day, price) tuples
    pub fn generate_path(&mut self, num_days: usize) -> Vec<(u32, f64)> {
        let dt: f64 = 1.0 / 252.0; // One trading day in years
        let mut prices = Vec::with_capacity(num_days);
        let mut current_price = self.initial_price;

        for day in 0..num_days {
            prices.push((day as u32, current_price));
            
            // GBM formula: dS = μS dt + σS dW
            // Discretized: S(t+dt) = S(t) * exp((μ - 0.5σ²)dt + σ√dt * Z)
            // Z ~ N(0,1) standard normal (can be negative!)
            let z: f64 = self.rng.sample(rand_distr::StandardNormal);
            let brownian_motion = z * dt.sqrt();
            
            let drift_term = (self.drift - 0.5 * self.volatility.powi(2)) * dt;
            let diffusion_term = self.volatility * brownian_motion;
            
            current_price *= (drift_term + diffusion_term).exp();
        }

        prices
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
