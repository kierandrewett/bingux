//! Single-use authentication tickets.
//!
//! A ticket is minted only when a PAM conversation ended in success. It binds
//! the account, the intent and the generation, expires after a short time and
//! is consumed by the first redemption attempt whether or not that attempt
//! passes the checks: a wrong guess burns it.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use bsm_proto::state::{AuthIntent, Generation};

/// Upper bound of outstanding tickets. Mint evicts the soonest-to-expire first.
const MAX_TICKETS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub uid: u32,
    pub user: String,
    pub intent: AuthIntent,
    pub generation: Generation,
    pub expires: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedeemError {
    Unknown,
    Expired,
    WrongGeneration,
    WrongIntent,
}

pub struct Tickets {
    ttl: Duration,
    map: HashMap<u64, Ticket>,
}

impl Tickets {
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, map: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Mint a ticket. `rand` supplies unguessable non-zero values from the
    /// kernel CSPRNG; collisions and zero are retried.
    pub fn mint(
        &mut self,
        now: Instant,
        uid: u32,
        user: &str,
        intent: AuthIntent,
        generation: Generation,
        mut rand: impl FnMut() -> u64,
    ) -> u64 {
        self.expire(now);
        while self.map.len() >= MAX_TICKETS {
            let oldest = self.map.iter().min_by_key(|(_, t)| t.expires).map(|(k, _)| *k);
            match oldest {
                Some(k) => {
                    self.map.remove(&k);
                }
                None => break,
            }
        }
        let id = loop {
            let id = rand();
            if id != 0 && !self.map.contains_key(&id) {
                break id;
            }
        };
        self.map.insert(
            id,
            Ticket { uid, user: user.to_owned(), intent, generation, expires: now + self.ttl },
        );
        id
    }

    /// Consume the ticket and check it. The ticket is gone afterwards in every case.
    pub fn redeem(
        &mut self,
        now: Instant,
        id: u64,
        generation: Generation,
        intent: AuthIntent,
    ) -> Result<Ticket, RedeemError> {
        let t = self.map.remove(&id).ok_or(RedeemError::Unknown)?;
        if now >= t.expires {
            return Err(RedeemError::Expired);
        }
        if t.generation != generation {
            return Err(RedeemError::WrongGeneration);
        }
        if t.intent != intent {
            return Err(RedeemError::WrongIntent);
        }
        Ok(t)
    }

    pub fn expire(&mut self, now: Instant) {
        self.map.retain(|_, t| now < t.expires);
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Earliest expiry, for the event loop timeout.
    pub fn next_expiry(&self) -> Option<Instant> {
        self.map.values().map(|t| t.expires).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq() -> impl FnMut() -> u64 {
        let mut n = 0;
        move || {
            n += 1;
            n
        }
    }

    #[test]
    fn redeem_succeeds_once() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let id = s.mint(t0, 1000, "alice", AuthIntent::Login, 7, seq());
        let t = s.redeem(t0, id, 7, AuthIntent::Login).unwrap();
        assert_eq!((t.uid, t.user.as_str(), t.generation), (1000, "alice", 7));
        assert_eq!(s.redeem(t0, id, 7, AuthIntent::Login), Err(RedeemError::Unknown));
    }

    #[test]
    fn expired_ticket_is_rejected_and_consumed() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let id = s.mint(t0, 1000, "a", AuthIntent::Login, 1, seq());
        let late = t0 + Duration::from_secs(30);
        assert_eq!(s.redeem(late, id, 1, AuthIntent::Login), Err(RedeemError::Expired));
        assert_eq!(s.redeem(t0, id, 1, AuthIntent::Login), Err(RedeemError::Unknown));
    }

    #[test]
    fn wrong_generation_or_intent_burns_the_ticket() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let id = s.mint(t0, 1000, "a", AuthIntent::Login, 1, seq());
        assert_eq!(s.redeem(t0, id, 2, AuthIntent::Login), Err(RedeemError::WrongGeneration));
        assert_eq!(s.redeem(t0, id, 1, AuthIntent::Login), Err(RedeemError::Unknown));
        let id = s.mint(t0, 1000, "a", AuthIntent::Unlock, 1, seq());
        assert_eq!(s.redeem(t0, id, 1, AuthIntent::Login), Err(RedeemError::WrongIntent));
        assert_eq!(s.redeem(t0, id, 1, AuthIntent::Unlock), Err(RedeemError::Unknown));
    }

    #[test]
    fn zero_and_colliding_values_are_never_issued() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let mut values = vec![5u64, 5, 0, 6].into_iter();
        let mut rand = move || values.next().unwrap();
        let a = s.mint(t0, 1, "a", AuthIntent::Login, 1, &mut rand);
        let b = s.mint(t0, 2, "b", AuthIntent::Login, 1, &mut rand);
        assert_eq!((a, b), (5, 6));
    }

    #[test]
    fn outstanding_tickets_are_bounded() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let mut r = seq();
        for i in 0..200 {
            s.mint(t0 + Duration::from_millis(i), 1, "a", AuthIntent::Login, 1, &mut r);
        }
        assert!(s.len() <= MAX_TICKETS);
    }
}
