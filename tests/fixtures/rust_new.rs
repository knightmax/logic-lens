// AI-modified Rust file
use std::collections::HashMap;

pub fn calculate_total(items: &[Item]) -> f64 {
    items
        .iter()
        .filter(|i| i.quantity > 0)
        .map(|i| i.price * i.quantity as f64)
        .sum()
}

fn apply_tax(amount: f64, rate: f64) -> f64 {
    amount * (1.0 + rate)
}

pub struct Item {
    pub name: String,
    pub price: f64,
    pub quantity: u32,
}

impl Item {
    pub fn new(name: &str, price: f64, quantity: u32) -> Self {
        Item {
            name: name.to_string(),
            price,
            quantity,
        }
    }

    pub fn total(&self) -> f64 {
        self.price * self.quantity as f64
    }
}
