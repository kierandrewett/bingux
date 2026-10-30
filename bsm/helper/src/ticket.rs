//! Single-use authentication tickets.
//!
//! A ticket is minted only when a PAM conversation ended in success. It binds
//! the account, the intent and the generation, expires after a short time and
//! is consumed by the first redemption attempt whether or not that attempt
//! passes the checks: a wrong guess burns it.
//!
//! A ticket carries a payload `T`. In the helper that is the parked
//! authentication child that holds the authenticated PAM handle; dropping the
//! payload (expiry, eviction, a failed redemption) is what ends that child.

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

pub struct Tickets<T> {
    ttl: Duration,
    map: HashMap<u64, (Ticket, T)>,
}

impl<T> Tickets<T> {
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
    #[allow(clippy::too_many_arguments)]
    pub fn mint(
        &mut self,
        now: Instant,
        uid: u32,
        user: &str,
        intent: AuthIntent,
        generation: Generation,
        payload: T,
        mut rand: impl FnMut() -> u64,
    ) -> u64 {
        self.expire(now);
        while self.map.len() >= MAX_TICKETS {
            let oldest = self.map.iter().min_by_key(|(_, (t, _))| t.expires).map(|(k, _)| *k);
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
            (Ticket { uid, user: user.to_owned(), intent, generation, expires: now + self.ttl }, payload),
        );
        id
    }

    /// Consume the ticket and check it. The ticket and its payload are gone
    /// afterwards in every case; only a passing redemption hands the payload back.
    pub fn redeem(
        &mut self,
        now: Instant,
        id: u64,
        generation: Generation,
        intents: &[AuthIntent],
    ) -> Result<(Ticket, T), RedeemError> {
        let (t, payload) = self.map.remove(&id).ok_or(RedeemError::Unknown)?;
        if now >= t.expires {
            return Err(RedeemError::Expired);
        }
        if t.generation != generation {
            return Err(RedeemError::WrongGeneration);
        }
        if !intents.contains(&t.intent) {
            return Err(RedeemError::WrongIntent);
        }
        Ok((t, payload))
    }

    pub fn expire(&mut self, now: Instant) {
        self.map.retain(|_, (t, _)| now < t.expires);
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Earliest expiry, for the event loop timeout.
    pub fn next_expiry(&self) -> Option<Instant> {
        self.map.values().map(|(t, _)| t.expires).min()
    }

    /// Mutable access to every payload (to mark parked children that were reaped).
    pub fn payloads_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.map.values_mut().map(|(_, p)| p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn seq() -> impl FnMut() -> u64 {
        let mut n = 0;
        move || {
            n += 1;
            n
        }
    }

    const LOGIN: &[AuthIntent] = &[AuthIntent::Login];

    #[test]
    fn redeem_succeeds_once() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let id = s.mint(t0, 1000, "alice", AuthIntent::Login, 7, (), seq());
        let (t, ()) = s.redeem(t0, id, 7, LOGIN).unwrap();
        assert_eq!((t.uid, t.user.as_str(), t.generation), (1000, "alice", 7));
        assert_eq!(s.redeem(t0, id, 7, LOGIN).map(|_| ()), Err(RedeemError::Unknown));
    }

    #[test]
    fn expired_ticket_is_rejected_and_consumed() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let id = s.mint(t0, 1000, "a", AuthIntent::Login, 1, (), seq());
        let late = t0 + Duration::from_secs(30);
        assert_eq!(s.redeem(late, id, 1, LOGIN).map(|_| ()), Err(RedeemError::Expired));
        assert_eq!(s.redeem(t0, id, 1, LOGIN).map(|_| ()), Err(RedeemError::Unknown));
    }

    #[test]
    fn wrong_generation_or_intent_burns_the_ticket() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let id = s.mint(t0, 1000, "a", AuthIntent::Login, 1, (), seq());
        assert_eq!(s.redeem(t0, id, 2, LOGIN).map(|_| ()), Err(RedeemError::WrongGeneration));
        assert_eq!(s.redeem(t0, id, 1, LOGIN).map(|_| ()), Err(RedeemError::Unknown));
        let id = s.mint(t0, 1000, "a", AuthIntent::Unlock, 1, (), seq());
        assert_eq!(s.redeem(t0, id, 1, LOGIN).map(|_| ()), Err(RedeemError::WrongIntent));
        assert_eq!(s.redeem(t0, id, 1, &[AuthIntent::Unlock]).map(|_| ()), Err(RedeemError::Unknown));
    }

    #[test]
    fn several_intents_can_be_accepted() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let id = s.mint(t0, 1, "a", AuthIntent::Switch, 1, (), seq());
        assert!(s.redeem(t0, id, 1, &[AuthIntent::Login, AuthIntent::Switch]).is_ok());
    }

    #[test]
    fn zero_and_colliding_values_are_never_issued() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let mut values = vec![5u64, 5, 0, 6].into_iter();
        let mut rand = move || values.next().unwrap();
        let a = s.mint(t0, 1, "a", AuthIntent::Login, 1, (), &mut rand);
        let b = s.mint(t0, 2, "b", AuthIntent::Login, 1, (), &mut rand);
        assert_eq!((a, b), (5, 6));
    }

    #[test]
    fn outstanding_tickets_are_bounded() {
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let mut r = seq();
        for i in 0..200 {
            s.mint(t0 + Duration::from_millis(i), 1, "a", AuthIntent::Login, 1, (), &mut r);
        }
        assert!(s.len() <= MAX_TICKETS);
    }

    struct Probe(Rc<Cell<u32>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn payloads_are_released_on_every_way_a_ticket_can_end() {
        let drops = Rc::new(Cell::new(0));
        let t0 = Instant::now();
        let mut s = Tickets::new(Duration::from_secs(30));
        let mut r = seq();
        // Failed redemption.
        let id = s.mint(t0, 1, "a", AuthIntent::Login, 1, Probe(drops.clone()), &mut r);
        let _ = s.redeem(t0, id, 2, LOGIN);
        assert_eq!(drops.get(), 1);
        // Expiry.
        s.mint(t0, 1, "a", AuthIntent::Login, 1, Probe(drops.clone()), &mut r);
        s.expire(t0 + Duration::from_secs(31));
        assert_eq!(drops.get(), 2);
        // Clear.
        s.mint(t0, 1, "a", AuthIntent::Login, 1, Probe(drops.clone()), &mut r);
        s.clear();
        assert_eq!(drops.get(), 3);
        // A passing redemption hands the payload to the caller instead.
        let id = s.mint(t0, 1, "a", AuthIntent::Login, 1, Probe(drops.clone()), &mut r);
        let (_, payload) = s.redeem(t0, id, 1, LOGIN).unwrap();
        assert_eq!(drops.get(), 3);
        drop(payload);
        assert_eq!(drops.get(), 4);
    }
}
