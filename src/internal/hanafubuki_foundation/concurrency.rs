//! Small concurrency primitives shared by coordinators, never domain state machines.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Revisioned<T> {
    revision: u64,
    value: T,
}

impl<T> Revisioned<T> {
    pub fn new(value: T) -> Self {
        Self { revision: 0, value }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn commit(&mut self, value: T) -> u64 {
        self.revision = self.revision.checked_add(1).expect("revision exhausted");
        self.value = value;
        self.revision
    }

    pub fn touch(&mut self) -> u64
    where
        T: Clone,
    {
        self.commit(self.value.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenerationToken(u64);

impl GenerationToken {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Default)]
pub struct GenerationSequence(AtomicU64);

impl GenerationSequence {
    pub fn starting_at(next: u64) -> Self {
        Self(AtomicU64::new(next))
    }

    pub fn next(&self) -> GenerationToken {
        GenerationToken(self.0.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct BoundedHistory<T> {
    capacity: usize,
    entries: VecDeque<T>,
}

impl<T> BoundedHistory<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
        }
    }

    pub fn push(&mut self, value: T) -> Option<T> {
        self.entries.push_back(value);
        (self.entries.len() > self.capacity)
            .then(|| self.entries.pop_front())
            .flatten()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.iter()
    }

    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.entries.contains(value)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Clone, Copy, Debug)]
pub struct WaitPolicy {
    pub initial: Duration,
    pub maximum: Duration,
    pub multiplier: u32,
}

impl WaitPolicy {
    pub fn next_delay(&self, current: Duration) -> Duration {
        current
            .saturating_mul(self.multiplier)
            .clamp(self.initial, self.maximum)
    }
}

#[cfg(test)]
mod tests;
