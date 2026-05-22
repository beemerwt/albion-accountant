use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::transaction::MarketTransaction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub struct CorrelatedTradeCandidate {
    pub side: TradeSide,
    pub order_id: u64,
    pub location: String,
    pub item_type_id: String,
    pub unit_price_silver: u64,
    pub amount: u32,
    pub observed_at: Instant,
}

#[derive(Debug, Clone)]
pub struct MarketOrderCacheEntry {
    pub order_id: u64,
    pub location: String,
    pub item_type_id: String,
    pub unit_price_silver: u64,
    pub observed_at: Instant,
}

#[derive(Debug, Clone)]
pub struct PendingTrade {
    pub side: TradeSide,
    pub order_id: u64,
    pub amount: u32,
    pub created_at: Instant,
}

#[derive(Debug, Default, Clone)]
pub struct CorrelatorStats {
    pub orders_cached: usize,
    pub pending_created: usize,
    pub pending_confirmed: usize,
    pub pending_failed: usize,
    pub pending_expired: usize,
    pub cache_expired: usize,
}

pub struct TradeCorrelator {
    order_cache: VecDeque<MarketOrderCacheEntry>,
    pending: VecDeque<PendingTrade>,
    max_order_cache: usize,
    max_pending: usize,
    order_ttl: Duration,
    pending_ttl: Duration,
    stats: CorrelatorStats,
}

impl Default for TradeCorrelator {
    fn default() -> Self {
        Self::new(2000, 512, Duration::from_secs(180), Duration::from_secs(30))
    }
}

impl TradeCorrelator {
    pub fn new(
        max_order_cache: usize,
        max_pending: usize,
        order_ttl: Duration,
        pending_ttl: Duration,
    ) -> Self {
        Self {
            order_cache: VecDeque::new(),
            pending: VecDeque::new(),
            max_order_cache: max_order_cache.max(1),
            max_pending: max_pending.max(1),
            order_ttl,
            pending_ttl,
            stats: CorrelatorStats::default(),
        }
    }

    pub fn observe_market_orders(&mut self, orders: impl IntoIterator<Item = MarketOrderCacheEntry>) {
        for mut order in orders {
            order.observed_at = Instant::now();
            if let Some(existing) = self.order_cache.iter_mut().find(|x| x.order_id == order.order_id) {
                *existing = order;
            } else {
                if self.order_cache.len() >= self.max_order_cache {
                    self.order_cache.pop_front();
                }
                self.order_cache.push_back(order);
                self.stats.orders_cached = self.stats.orders_cached.wrapping_add(1);
            }
        }
    }

    pub fn observe_buy_request(&mut self, order_id: u64, amount: u32) {
        self.push_pending(PendingTrade {
            side: TradeSide::Buy,
            order_id,
            amount,
            created_at: Instant::now(),
        });
    }

    pub fn observe_sell_request(&mut self, order_id: u64, amount: u32) {
        self.push_pending(PendingTrade {
            side: TradeSide::Sell,
            order_id,
            amount,
            created_at: Instant::now(),
        });
    }

    pub fn observe_buy_response(&mut self, success: bool) -> Option<MarketTransaction> {
        self.observe_response(TradeSide::Buy, success)
    }

    pub fn observe_sell_response(&mut self, success: bool) -> Option<MarketTransaction> {
        self.observe_response(TradeSide::Sell, success)
    }

    pub fn expire_old_state(&mut self) {
        let now = Instant::now();
        let before_orders = self.order_cache.len();
        self.order_cache
            .retain(|x| now.duration_since(x.observed_at) <= self.order_ttl);
        self.stats.cache_expired = self
            .stats
            .cache_expired
            .wrapping_add(before_orders.saturating_sub(self.order_cache.len()));

        let before_pending = self.pending.len();
        self.pending
            .retain(|x| now.duration_since(x.created_at) <= self.pending_ttl);
        self.stats.pending_expired = self
            .stats
            .pending_expired
            .wrapping_add(before_pending.saturating_sub(self.pending.len()));
    }

    pub fn stats(&self) -> &CorrelatorStats {
        &self.stats
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn cache_len(&self) -> usize {
        self.order_cache.len()
    }

    pub fn has_cached_order(&self, order_id: u64) -> bool {
        self.order_cache.iter().any(|o| o.order_id == order_id)
    }

    fn push_pending(&mut self, pending: PendingTrade) {
        if self.pending.len() >= self.max_pending {
            self.pending.pop_front();
        }
        self.pending.push_back(pending);
        self.stats.pending_created = self.stats.pending_created.wrapping_add(1);
    }

    fn observe_response(&mut self, side: TradeSide, success: bool) -> Option<MarketTransaction> {
        let idx = self.pending.iter().rposition(|p| p.side == side)?;
        let pending = self.pending.remove(idx)?;

        if !success {
            self.stats.pending_failed = self.stats.pending_failed.wrapping_add(1);
            return None;
        }

        let order = self.order_cache.iter().rfind(|o| o.order_id == pending.order_id)?;
        let candidate = CorrelatedTradeCandidate {
            side: pending.side,
            order_id: pending.order_id,
            location: order.location.clone(),
            item_type_id: order.item_type_id.clone(),
            unit_price_silver: order.unit_price_silver,
            amount: pending.amount,
            observed_at: Instant::now(),
        };
        let tx = MarketTransaction::new(
            candidate.location,
            candidate.item_type_id,
            candidate.amount,
            candidate.unit_price_silver,
            None,
        )
        .ok()?;

        self.stats.pending_confirmed = self.stats.pending_confirmed.wrapping_add(1);
        Some(tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_order(id: u64) -> MarketOrderCacheEntry {
        MarketOrderCacheEntry {
            order_id: id,
            location: "Bridgewatch".to_string(),
            item_type_id: "T4_BAG".to_string(),
            unit_price_silver: 1200,
            observed_at: Instant::now(),
        }
    }

    #[test]
    fn confirms_buy_trade_from_cached_order() {
        let mut c = TradeCorrelator::default();
        c.observe_market_orders([sample_order(42)]);
        c.observe_buy_request(42, 3);

        let tx = c.observe_buy_response(true).expect("confirmed tx");
        assert_eq!(tx.location, "Bridgewatch");
        assert_eq!(tx.item, "T4_BAG");
        assert_eq!(tx.quantity, 3);
        assert_eq!(tx.total_cost, 3600);
    }

    #[test]
    fn failed_response_drops_pending() {
        let mut c = TradeCorrelator::default();
        c.observe_market_orders([sample_order(42)]);
        c.observe_sell_request(42, 2);

        assert!(c.observe_sell_response(false).is_none());
        assert_eq!(c.pending_len(), 0);
        assert_eq!(c.stats().pending_failed, 1);
    }
}
