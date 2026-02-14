//! Trading Calendar for /CL Oil Futures
//!
//! /CL trades 23 hours/day, 5 days/week (Sun-Fri)
//! - Sunday 18:00 ET to Friday 17:00 ET (continuous)
//! - Daily maintenance: 17:00-18:00 ET
//! - Weekend: Friday 17:00 - Sunday 18:00

/// Minutes in a day (24 hours)
pub const MINUTES_PER_DAY: u32 = 24 * 60;
/// Trading day start in minutes (18:00 = 1080 minutes)
pub const TRADING_DAY_START: u32 = 18 * 60;
/// Trading day end in minutes (17:00 = 1020 minutes of next day)
pub const TRADING_DAY_END: u32 = 17 * 60;
/// Daily maintenance window start (17:00)
pub const MAINTENANCE_START: u32 = 17 * 60;
/// Daily maintenance window end (18:00)
pub const MAINTENANCE_END: u32 = 18 * 60;

/// Simple timestamp representation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    /// Days since simulation start (Day 0 = start)
    pub day: u32,
    /// Minutes from midnight (0-1439)
    pub minute: u32,
}

impl Timestamp {
    /// Create new timestamp
    pub fn new(day: u32, minute: u32) -> Self {
        Self { day, minute }
    }

    /// Get total minutes since start (for calculations)
    pub fn total_minutes(&self) -> u64 {
        self.day as u64 * MINUTES_PER_DAY as u64 + self.minute as u64
    }

    /// Format as human-readable string
    pub fn format(&self) -> String {
        let hours = self.minute / 60;
        let mins = self.minute % 60;
        let weekday = match self.day % 7 {
            0 => "Mon", 1 => "Tue", 2 => "Wed", 3 => "Thu",
            4 => "Fri", 5 => "Sat", 6 => "Sun", _ => "???",
        };
        let week = self.day / 7;
        format!("Day {} ({} W{}) {:02}:{:02}", self.day, weekday, week, hours, mins)
    }

    /// Format just the time
    pub fn format_time(&self) -> String {
        let hours = self.minute / 60;
        let mins = self.minute % 60;
        format!("{:02}:{:02}", hours, mins)
    }
}

/// Trading calendar for /CL futures
#[derive(Debug, Clone)]
pub struct TradingCalendar;

impl TradingCalendar {
    /// Create new trading calendar
    pub fn new() -> Self {
        Self
    }

    /// Check if a day is a trading day (not Saturday)
    pub fn is_trading_day(&self, day: u32) -> bool {
        // Day 0 = Monday, so day % 7 gives:
        // 0=Mon, 1=Tue, 2=Wed, 3=Thu, 4=Fri, 5=Sat, 6=Sun
        day % 7 != 5 // Saturday is not a trading day
    }

    /// Check if timestamp is within trading hours
    pub fn is_trading_time(&self, timestamp: &Timestamp) -> bool {
        let weekday = timestamp.day % 7;
        let minute = timestamp.minute;
        
        match weekday {
            // Friday: trading until 17:00 (1020 minutes)
            4 => minute < MAINTENANCE_START,
            // Saturday: no trading
            5 => false,
            // Sunday: trading from 18:00 (1080 minutes)
            6 => minute >= MAINTENANCE_END,
            // Mon-Thu: trading except 17:00-18:00
            _ => minute < MAINTENANCE_START || minute >= MAINTENANCE_END,
        }
    }

    /// Get next trading timestamp
    pub fn next_trading_time(&self, current: &Timestamp, interval_minutes: u32) -> Timestamp {
        let mut next = Timestamp::new(
            current.day,
            current.minute + interval_minutes
        );
        
        // Handle minute overflow to next day
        if next.minute >= MINUTES_PER_DAY {
            next.day += next.minute / MINUTES_PER_DAY;
            next.minute = next.minute % MINUTES_PER_DAY;
        }
        
        // Keep advancing until we find a trading time
        while !self.is_trading_time(&next) {
            next.minute += interval_minutes;
            if next.minute >= MINUTES_PER_DAY {
                next.day += next.minute / MINUTES_PER_DAY;
                next.minute = next.minute % MINUTES_PER_DAY;
            }
        }
        
        next
    }

    /// Generate sequence of trading timestamps
    /// 
    /// # Arguments
    /// * `start_day` - Starting day
    /// * `start_minute` - Starting minute of day
    /// * `num_bars` - Number of bars to generate
    /// * `interval_minutes` - Interval between bars (typically 10)
    /// 
    /// # Returns
    /// Vector of valid trading timestamps
    pub fn generate_trading_times(
        &self,
        start_day: u32,
        start_minute: u32,
        num_bars: usize,
        interval_minutes: u32,
    ) -> Vec<Timestamp> {
        let mut times = Vec::with_capacity(num_bars);
        let mut current = Timestamp::new(start_day, start_minute);

        // If start is not a trading time, advance to first valid time
        while !self.is_trading_time(&current) {
            current = self.next_trading_time(&current, interval_minutes);
        }

        for _ in 0..num_bars {
            times.push(current);
            current = self.next_trading_time(&current, interval_minutes);
        }

        times
    }

    /// Calculate fractional days between two timestamps
    /// 
    /// Used for DTE calculation in option pricing
    pub fn fractional_days_between(&self, from: &Timestamp, to: &Timestamp) -> f64 {
        let from_minutes = from.total_minutes();
        let to_minutes = to.total_minutes();
        (to_minutes as f64 - from_minutes as f64) / (24.0 * 60.0)
    }

    /// Calculate DTE from current timestamp to expiration day
    pub fn calculate_dte(&self, current: &Timestamp, expiration_day: u32) -> f64 {
        if current.day > expiration_day {
            return 0.0;
        }
        if current.day == expiration_day {
            // On expiration day, DTE is based on time remaining
            // Assume expiration at 14:30 (870 minutes)
            let expiration_minute = 14 * 60 + 30;
            if current.minute >= expiration_minute {
                return 0.0;
            }
            return (expiration_minute - current.minute) as f64 / (24.0 * 60.0);
        }
        // Count trading days between
        let mut trading_days = 0;
        for day in current.day..expiration_day {
            if self.is_trading_day(day) {
                trading_days += 1;
            }
        }
        trading_days as f64
    }

    /// Get the next trading day after the given day
    pub fn next_trading_day(&self, day: u32) -> u32 {
        let mut candidate = day + 1;
        while !self.is_trading_day(candidate) {
            candidate += 1;
        }
        candidate
    }

    /// Count trading days between two days (exclusive of end)
    pub fn trading_days_between(&self, start: u32, end: u32) -> u32 {
        (start..end).filter(|&d| self.is_trading_day(d)).count() as u32
    }
}

impl Default for TradingCalendar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trading_hours_weekday() {
        let cal = TradingCalendar::new();
        
        // Monday 10:00 - should be trading
        let mon_10 = Timestamp::new(0, 10 * 60);
        assert!(cal.is_trading_time(&mon_10));
        
        // Monday 17:30 - maintenance window, NOT trading
        let mon_1730 = Timestamp::new(0, 17 * 60 + 30);
        assert!(!cal.is_trading_time(&mon_1730));
        
        // Friday 16:00 - should be trading
        let fri_16 = Timestamp::new(4, 16 * 60);
        assert!(cal.is_trading_time(&fri_16));
        
        // Friday 18:00 - weekend, NOT trading
        let fri_18 = Timestamp::new(4, 18 * 60);
        assert!(!cal.is_trading_time(&fri_18));
        
        // Sunday 19:00 - should be trading
        let sun_19 = Timestamp::new(6, 19 * 60);
        assert!(cal.is_trading_time(&sun_19));
    }

    #[test]
    fn test_generate_trading_times() {
        let cal = TradingCalendar::new();
        
        // Generate 10 bars at 10-min intervals starting Monday 9:00
        let times = cal.generate_trading_times(0, 9 * 60, 10, 10);
        
        assert_eq!(times.len(), 10);
        // First bar should be 9:00
        assert_eq!(times[0].minute, 9 * 60);
        
        // Should skip the 17:00-18:00 maintenance window
        // 9:00 + 10*10min = 10:40, still well before 17:00
        assert_eq!(times[9].minute, 10 * 60 + 30);
    }

    #[test]
    fn test_fractional_days() {
        let cal = TradingCalendar::new();
        let t1 = Timestamp::new(0, 10 * 60);
        let t2 = Timestamp::new(1, 10 * 60);
        
        let days = cal.fractional_days_between(&t1, &t2);
        assert!((days - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_calculate_dte() {
        let cal = TradingCalendar::new();
        
        // From Monday 10:00 to Friday 14:30 (expiration)
        let current = Timestamp::new(0, 10 * 60);
        let dte = cal.calculate_dte(&current, 4);
        
        // Should be approximately 4 trading days
        assert!(dte > 3.5 && dte < 4.5);
    }
}
