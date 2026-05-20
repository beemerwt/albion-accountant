use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MarketTransaction {
    pub location: String,
    pub item: String,
    pub quantity: u32,
    pub per_item_cost: u64,
    pub total_cost: u64,
}

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("quantity must be positive")]
    ZeroQuantity,
    #[error("overflow computing total cost")]
    Overflow,
}

impl MarketTransaction {
    pub fn new(
        location: String,
        item: String,
        quantity: u32,
        per_item_cost: u64,
        total_cost: Option<u64>,
    ) -> Result<Self, TransactionError> {
        if quantity == 0 {
            return Err(TransactionError::ZeroQuantity);
        }
        let computed = u64::from(quantity)
            .checked_mul(per_item_cost)
            .ok_or(TransactionError::Overflow)?;
        Ok(Self {
            location,
            item,
            quantity,
            per_item_cost,
            total_cost: total_cost.unwrap_or(computed),
        })
    }

    pub fn dedupe_key(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.location, self.item, self.quantity, self.per_item_cost, self.total_cost
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_total() {
        let tx = MarketTransaction::new("Martlock".into(), "T4_BAG".into(), 3, 1250, None).unwrap();
        assert_eq!(tx.total_cost, 3750);
    }
}
