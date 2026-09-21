use std::collections::VecDeque;

use slab::Slab;

use crate::{
    orders::OrderPointer,
    types::{Price, Quantity},
};

pub struct LevelInfo {
    pub price: Price,
    pub quantity: Quantity,
}

pub type LevelInfos = Vec<LevelInfo>;

pub struct OrderBookLevelInfos {
    pub bids: LevelInfos,
    pub asks: LevelInfos,
}

impl OrderBookLevelInfos {
    pub fn get_bids(&self) -> &LevelInfos {
        &self.bids
    }
    pub fn get_asks(&self) -> &LevelInfos {
        &self.asks
    }
}

pub struct PriceLevel {
    pub orders: Slab<OrderPointer>,
    pub queue: VecDeque<usize>,
}

impl PriceLevel {
    pub fn new() -> Self {
        Self {
            orders: Slab::new(),
            queue: VecDeque::new(),
        }
    }
    pub fn insert(&mut self, order: OrderPointer) -> usize {
        let key = self.orders.insert(order);
        self.queue.push_back(key);
        key
    }
    pub fn remove(&mut self, key: usize) {
        self.orders.remove(key);
        self.queue.retain(|k| *k != key);
    }
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
    pub fn front(&self) -> Option<&OrderPointer> {
        let key = self.queue.front()?;
        Some(&self.orders[*key])
    }
    pub fn pop_front(&mut self) -> Option<OrderPointer> {
        let key = self.queue.pop_front()?;
        Some(self.orders.remove(key))
    }
}

impl Default for PriceLevel {
    fn default() -> Self {
        Self::new()
    }
}
