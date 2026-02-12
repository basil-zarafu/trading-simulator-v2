//! Option Pricing Models
//!
//! Black-Scholes for stocks, Black-76 for futures options (/CL)

/// Standard normal cumulative distribution function
fn norm_cdf(x: f64) -> f64 {
    (1.0 + erf(x / std::f64::consts::SQRT_2)) / 2.0
}

/// Error function approximation (Abramowitz and Stegun)
fn erf(x: f64) -> f64 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

    sign * y
}

/// Greeks for an option
#[derive(Debug, Clone, Copy)]
pub struct Greeks {
    pub delta: f64,
    pub gamma: f64,
    pub theta: f64,
    pub vega: f64,
    pub rho: f64,
}

/// Black-76 model for futures options (used for /CL)
///
/// Black-76 is like Black-Scholes but uses the futures price directly
/// instead of spot price and risk-free rate.
pub struct Black76;

impl Black76 {
    /// Price a European option on a futures contract
    ///
    /// # Arguments
    /// * `futures_price` - Current futures price (F)
    /// * `strike` - Strike price (K)
    /// * `time_to_expiry` - Time to expiry in years (T)
    /// * `risk_free_rate` - Risk-free rate (r)
    /// * `volatility` - Annualized volatility (σ)
    /// * `is_call` - true for call, false for put
    ///
    /// # Returns
    /// Option price
    pub fn price(
        futures_price: f64,
        strike: f64,
        time_to_expiry: f64,
        risk_free_rate: f64,
        volatility: f64,
        is_call: bool,
    ) -> f64 {
        if time_to_expiry <= 0.0 {
            // At expiry, intrinsic value only
            let intrinsic = if is_call {
                (futures_price - strike).max(0.0)
            } else {
                (strike - futures_price).max(0.0)
            };
            return intrinsic;
        }

        let d1 = Self::d1(futures_price, strike, time_to_expiry, volatility);
        let d2 = Self::d2(futures_price, strike, time_to_expiry, volatility);

        let discount = (-risk_free_rate * time_to_expiry).exp();

        if is_call {
            discount * (futures_price * norm_cdf(d1) - strike * norm_cdf(d2))
        } else {
            discount * (strike * norm_cdf(-d2) - futures_price * norm_cdf(-d1))
        }
    }

    /// Calculate Greeks for a futures option
    pub fn greeks(
        futures_price: f64,
        strike: f64,
        time_to_expiry: f64,
        risk_free_rate: f64,
        volatility: f64,
        is_call: bool,
    ) -> Greeks {
        if time_to_expiry <= 0.0 {
            return Greeks {
                delta: if is_call {
                    if futures_price > strike { 1.0 } else { 0.0 }
                } else {
                    if futures_price < strike { -1.0 } else { 0.0 }
                },
                gamma: 0.0,
                theta: 0.0,
                vega: 0.0,
                rho: 0.0,
            };
        }

        let d1 = Self::d1(futures_price, strike, time_to_expiry, volatility);
        let d2 = Self::d2(futures_price, strike, time_to_expiry, volatility);
        let discount = (-risk_free_rate * time_to_expiry).exp();

        // Delta
        let delta = if is_call {
            discount * norm_cdf(d1)
        } else {
            discount * (norm_cdf(d1) - 1.0)
        };

        // Gamma (same for calls and puts)
        let gamma = discount * norm_pdf(d1) / (futures_price * volatility * time_to_expiry.sqrt());

        // Theta (per year, convert to per day by dividing by 365)
        let theta = if is_call {
            -futures_price * discount * norm_pdf(d1) * volatility / (2.0 * time_to_expiry.sqrt())
                - risk_free_rate * strike * discount * norm_cdf(d2)
        } else {
            -futures_price * discount * norm_pdf(d1) * volatility / (2.0 * time_to_expiry.sqrt())
                + risk_free_rate * strike * discount * norm_cdf(-d2)
        };

        // Vega (per 1% change in volatility)
        let vega = futures_price * discount * norm_pdf(d1) * time_to_expiry.sqrt() / 100.0;

        // Rho
        let rho = if is_call {
            -time_to_expiry * discount * (futures_price * norm_cdf(d1) - strike * norm_cdf(d2))
        } else {
            -time_to_expiry * discount * (strike * norm_cdf(-d2) - futures_price * norm_cdf(-d1))
        };

        Greeks {
            delta,
            gamma,
            theta: theta / 365.0, // Convert to per day
            vega,
            rho,
        }
    }

    /// Calculate implied volatility from market price
    ///
    /// Uses Newton-Raphson iteration
    pub fn implied_volatility(
        market_price: f64,
        futures_price: f64,
        strike: f64,
        time_to_expiry: f64,
        risk_free_rate: f64,
        is_call: bool,
    ) -> Option<f64> {
        let mut vol = 0.3; // Initial guess: 30%
        let max_iterations = 100;
        let tolerance = 1e-6;

        for _ in 0..max_iterations {
            let price = Self::price(futures_price, strike, time_to_expiry, risk_free_rate, vol, is_call);
            let diff = price - market_price;

            if diff.abs() < tolerance {
                return Some(vol);
            }

            let vega = Self::greeks(futures_price, strike, time_to_expiry, risk_free_rate, vol, is_call).vega * 100.0;

            if vega.abs() < 1e-10 {
                return None; // Vega too small, can't converge
            }

            vol -= diff / vega;

            if vol <= 0.0 {
                vol = 0.001; // Keep positive
            }
        }

        None // Failed to converge
    }

    fn d1(f: f64, k: f64, t: f64, sigma: f64) -> f64 {
        (f / k).ln() + (sigma.powi(2) / 2.0) * t / (sigma * t.sqrt())
    }

    fn d2(f: f64, k: f64, t: f64, sigma: f64) -> f64 {
        Self::d1(f, k, t, sigma) - sigma * t.sqrt()
    }
}

/// Standard normal probability density function
fn norm_pdf(x: f64) -> f64 {
    (-x * x / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

/// Black-Scholes for spot options (stocks)
pub struct BlackScholes;

impl BlackScholes {
    /// Price a European option on a stock
    pub fn price(
        spot_price: f64,
        strike: f64,
        time_to_expiry: f64,
        risk_free_rate: f64,
        dividend_yield: f64,
        volatility: f64,
        is_call: bool,
    ) -> f64 {
        if time_to_expiry <= 0.0 {
            let intrinsic = if is_call {
                (spot_price - strike).max(0.0)
            } else {
                (strike - spot_price).max(0.0)
            };
            return intrinsic;
        }

        let d1 = Self::d1(spot_price, strike, time_to_expiry, risk_free_rate, dividend_yield, volatility);
        let d2 = Self::d2(spot_price, strike, time_to_expiry, risk_free_rate, dividend_yield, volatility);

        if is_call {
            spot_price * (-dividend_yield * time_to_expiry).exp() * norm_cdf(d1)
                - strike * (-risk_free_rate * time_to_expiry).exp() * norm_cdf(d2)
        } else {
            strike * (-risk_free_rate * time_to_expiry).exp() * norm_cdf(-d2)
                - spot_price * (-dividend_yield * time_to_expiry).exp() * norm_cdf(-d1)
        }
    }

    fn d1(s: f64, k: f64, t: f64, r: f64, q: f64, sigma: f64) -> f64 {
        ((s / k).ln() + (r - q + sigma.powi(2) / 2.0) * t) / (sigma * t.sqrt())
    }

    fn d2(s: f64, k: f64, t: f64, r: f64, q: f64, sigma: f64) -> f64 {
        Self::d1(s, k, t, r, q, sigma) - sigma * t.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_black76_call() {
        // Known test case: F=100, K=100, T=1, r=0.05, sigma=0.2
        // Expected call price ≈ 7.46
        let price = Black76::price(100.0, 100.0, 1.0, 0.05, 0.2, true);
        assert!((price - 7.46).abs() < 0.1, "Expected ~7.46, got {}", price);
    }

    #[test]
    fn test_black76_put_call_parity() {
        // Call - Put = Discounted (F - K)
        let f = 100.0;
        let k = 100.0;
        let t = 0.5;
        let r = 0.05;
        let sigma = 0.25;

        let call = Black76::price(f, k, t, r, sigma, true);
        let put = Black76::price(f, k, t, r, sigma, false);
        let discount = (-r * t).exp();

        let parity_lhs = call - put;
        let parity_rhs = discount * (f - k);

        assert!((parity_lhs - parity_rhs).abs() < 1e-10);
    }

    #[test]
    fn test_greeks_sanity() {
        let greeks = Black76::greeks(100.0, 100.0, 0.5, 0.05, 0.25, true);

        // ATM call should have delta ~ 0.5
        assert!((greeks.delta - 0.5).abs() < 0.1);

        // Gamma should be positive
        assert!(greeks.gamma > 0.0);

        // Theta should be negative (time decay)
        assert!(greeks.theta < 0.0);

        // Vega should be positive
        assert!(greeks.vega > 0.0);
    }

    #[test]
    fn test_at_expiry() {
        // At expiry, option is worth intrinsic value only
        let call_itm = Black76::price(110.0, 100.0, 0.0, 0.05, 0.25, true);
        assert_eq!(call_itm, 10.0);

        let call_otm = Black76::price(90.0, 100.0, 0.0, 0.05, 0.25, true);
        assert_eq!(call_otm, 0.0);

        let put_itm = Black76::price(90.0, 100.0, 0.0, 0.05, 0.25, false);
        assert_eq!(put_itm, 10.0);
    }
}
