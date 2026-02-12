//! Event Types for Trading Simulator V2
//! 
//! All state changes are recorded as immutable events.
//! The current state is derived by replaying events in order.

use crate::calendar::{Day, TimeOfDay};

/// Unique identifier for a position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PositionId(pub u64);

/// Unique identifier for a leg within a position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LegId(pub u64);

/// Option type (Put or Call)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionType {
    Put,
    Call,
}

/// Side of a trade (Long or Short)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Long,
    Short,
}

/// Represents a single option contract specification
#[derive(Debug, Clone)]
pub struct OptionContract {
    pub underlying_price: f64,
    pub strike: f64,
    pub option_type: OptionType,
    pub side: Side,
    pub expiration_day: Day,
}

/// All possible events in the trading system
#[derive(Debug, Clone)]
pub enum Event {
    /// A new position was opened
    PositionOpened {
        position_id: PositionId,
        timestamp: (Day, TimeOfDay),
        legs: Vec<(LegId, OptionContract, f64)>, // (leg_id, contract, premium_received/paid)
    },
    
    /// A position was fully closed
    PositionClosed {
        position_id: PositionId,
        timestamp: (Day, TimeOfDay),
        close_premiums: Vec<(LegId, f64)>, // (leg_id, premium_paid/received)
        reason: CloseReason,
    },
    
    /// A single leg was rolled
    LegRolled {
        position_id: PositionId,
        leg_id: LegId,
        timestamp: (Day, TimeOfDay),
        /// The old contract that was closed
        old_contract: OptionContract,
        /// Premium paid/received to close old contract
        close_premium: f64,
        /// The new contract that was opened
        new_contract: OptionContract,
        /// Premium received/paid for new contract
        open_premium: f64,
        /// Why this roll happened
        trigger: RollTrigger,
    },
    
    /// A roll was attempted but rejected (for audit/debugging)
    RollRejected {
        position_id: PositionId,
        leg_id: LegId,
        timestamp: (Day, TimeOfDay),
        reason: String,
    },
}

/// Reason a position was closed
#[derive(Debug, Clone)]
pub enum CloseReason {
    Expiration,
    StopLoss,
    Manual,
    StrategyExit,
}

/// Reason a leg was rolled
#[derive(Debug, Clone)]
pub enum RollTrigger {
    /// Time-based roll (e.g., 14:00 trigger)
    TimeTrigger,
    /// DTE-based roll (e.g., roll when DTE <= 28)
    DteThreshold { remaining_dte: u32 },
    /// Profit target hit (e.g., 50% of max profit)
    ProfitTarget { profit_percent: f64 },
    /// Loss limit hit
    StopLoss { loss_percent: f64 },
    /// Price-based trigger (e.g., underlying moved X points)
    PriceMove { points_moved: f64 },
    /// Delta-based trigger (e.g., delta exceeded threshold)
    DeltaThreshold { delta: f64 },
}

impl Event {
    /// Get the timestamp of this event
    pub fn timestamp(&self) -> (Day, TimeOfDay) {
        match self {
            Event::PositionOpened { timestamp, .. } => *timestamp,
            Event::PositionClosed { timestamp, .. } => *timestamp,
            Event::LegRolled { timestamp, .. } => *timestamp,
            Event::RollRejected { timestamp, .. } => *timestamp,
        }
    }
    
    /// Get the position ID associated with this event
    pub fn position_id(&self) -> PositionId {
        match self {
            Event::PositionOpened { position_id, .. } => *position_id,
            Event::PositionClosed { position_id, .. } => *position_id,
            Event::LegRolled { position_id, .. } => *position_id,
            Event::RollRejected { position_id, .. } => *position_id,
        }
    }
}

/// An event store that maintains an append-only log of events
#[derive(Debug, Default)]
pub struct EventStore {
    events: Vec<Event>,
    next_position_id: u64,
    next_leg_id: u64,
}

impl EventStore {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            next_position_id: 1,
            next_leg_id: 1,
        }
    }
    
    /// Append an event to the store
    pub fn append(&mut self, event: Event) {
        self.events.push(event);
    }
    
    /// Get all events for a specific position
    pub fn events_for_position(&self, position_id: PositionId) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|e| e.position_id() == position_id)
            .collect()
    }
    
    /// Get all events in order
    pub fn all_events(&self) -> &[Event] {
        &self.events
    }
    
    /// Generate a new unique position ID
    pub fn next_position_id(&mut self) -> PositionId {
        let id = PositionId(self.next_position_id);
        self.next_position_id += 1;
        id
    }
    
    /// Generate a new unique leg ID
    pub fn next_leg_id(&mut self) -> LegId {
        let id = LegId(self.next_leg_id);
        self.next_leg_id += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_position_id_generation() {
        let mut store = EventStore::new();
        assert_eq!(store.next_position_id().0, 1);
        assert_eq!(store.next_position_id().0, 2);
    }
    
    #[test]
    fn test_event_store_append() {
        let mut store = EventStore::new();
        let pos_id = store.next_position_id();
        
        let event = Event::PositionOpened {
            position_id: pos_id,
            timestamp: (0, 0),
            legs: vec![],
        };
        
        store.append(event);
        assert_eq!(store.all_events().len(), 1);
    }
}
