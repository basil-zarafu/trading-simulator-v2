//! Synthetic Trading Calendar
//! 
//! A deterministic calendar for backtesting that starts from Day 0.
//! Day 0 = Monday, January 1, Year 0
//! 
//! Trading schedule (for /CL oil futures options):
//! - Trading days: Monday-Friday (no weekends)
//! - Expiration: 14:30 on trading days
//! - Roll trigger: 14:00 on trading days

pub mod intraday;

use std::collections::HashSet;

/// Trading day (0-indexed from Jan 1, Year 0)
pub type Day = u32;

/// Time of day in minutes from midnight (0-1439)
pub type TimeOfDay = u16;

/// A synthetic trading calendar for backtesting
#[derive(Debug, Clone)]
pub struct Calendar {
    /// Cache of trading days for fast lookup
    trading_days: HashSet<Day>,
    /// Roll trigger time (default: 14:00 = 840 minutes)
    roll_trigger_time: TimeOfDay,
    /// Expiration time (default: 14:30 = 870 minutes)
    expiration_time: TimeOfDay,
}

impl Calendar {
    /// Create a new calendar with default /CL settings
    pub fn new() -> Self {
        Self {
            trading_days: HashSet::new(),
            roll_trigger_time: 14 * 60,      // 14:00
            expiration_time: 14 * 60 + 30,   // 14:30
        }
    }

    /// Check if a day is a trading day (Monday-Friday, no holidays in synthetic calendar)
    pub fn is_trading_day(&self, day: Day) -> bool {
        // Day 0 = Monday, so day % 7 gives:
        // 0=Mon, 1=Tue, 2=Wed, 3=Thu, 4=Fri, 5=Sat, 6=Sun
        matches!(day % 7, 0..=4)
    }

    /// Get the next trading day after the given day
    pub fn next_trading_day(&self, day: Day) -> Day {
        let mut candidate = day + 1;
        while !self.is_trading_day(candidate) {
            candidate += 1;
        }
        candidate
    }

    /// Count trading days between two days (exclusive of end)
    pub fn trading_days_between(&self, start: Day, end: Day) -> u32 {
        (start..end).filter(|&d| self.is_trading_day(d)).count() as u32
    }

    /// Get the expiration datetime for a given day
    /// Returns (day, time_of_day) for 14:30 expiry
    pub fn expiration_datetime(&self, day: Day) -> (Day, TimeOfDay) {
        (day, self.expiration_time)
    }

    /// Get the roll trigger datetime for a given day
    /// Returns (day, time_of_day) for 14:00 roll trigger
    pub fn roll_trigger_datetime(&self, day: Day) -> (Day, TimeOfDay) {
        (day, self.roll_trigger_time)
    }

    /// Calculate DTE (days to expiration) from current day to expiration day
    pub fn calculate_dte(&self, current_day: Day, expiration_day: Day) -> u32 {
        if expiration_day <= current_day {
            return 0;
        }
        self.trading_days_between(current_day, expiration_day)
    }

    /// Find the expiration day that gives approximately target_dte from current_day
    pub fn expiration_for_dte(&self, current_day: Day, target_dte: u32) -> Day {
        let mut day = current_day;
        let mut trading_days_count = 0;
        
        while trading_days_count < target_dte {
            day = self.next_trading_day(day);
            trading_days_count += 1;
        }
        day
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_day_zero_is_monday() {
        let cal = Calendar::new();
        assert!(cal.is_trading_day(0)); // Monday
        assert!(cal.is_trading_day(4)); // Friday
        assert!(!cal.is_trading_day(5)); // Saturday
        assert!(!cal.is_trading_day(6)); // Sunday
        assert!(cal.is_trading_day(7)); // Next Monday
    }

    #[test]
    fn test_next_trading_day() {
        let cal = Calendar::new();
        assert_eq!(cal.next_trading_day(0), 1); // Mon -> Tue
        assert_eq!(cal.next_trading_day(4), 7); // Fri -> Mon
        assert_eq!(cal.next_trading_day(5), 7); // Sat -> Mon
    }

    #[test]
    fn test_dte_calculation() {
        let cal = Calendar::new();
        // Day 0 (Mon) to Day 7 (next Mon) = 5 trading days
        assert_eq!(cal.calculate_dte(0, 7), 5);
        // Day 0 to Day 4 (Fri) = 4 trading days
        assert_eq!(cal.calculate_dte(0, 4), 4);
    }

    #[test]
    fn test_expiration_for_dte() {
        let cal = Calendar::new();
        // From Monday (day 0), 5 DTE should land on next Friday (day 4)
        // Actually: 0->1(Tue), 1->2(Wed), 2->3(Thu), 3->4(Fri) = 4 trading days
        // Let me recalculate: 5 DTE means 5 trading days ahead
        // Day 0 (Mon) + 5 trading days = Day 7 (next Mon)? No wait...
        // Day 0=Mon, Day 1=Tue, Day 2=Wed, Day 3=Thu, Day 4=Fri (that's 4 days)
        // Day 7=Mon (5th trading day)
        let exp_day = cal.expiration_for_dte(0, 5);
        assert_eq!(cal.calculate_dte(0, exp_day), 5);
    }
}
