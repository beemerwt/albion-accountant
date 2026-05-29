use crate::{
    error::{DecodeError, Result},
    trades::{TradeOperation, TradeRecord},
};
use chrono::{DateTime, Local};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const DEFAULT_PAGE: u32 = 1;
const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 250;

#[derive(Clone)]
pub struct TradeStore {
    connection: Arc<Mutex<Connection>>,
}

#[derive(Clone, Debug, Default)]
pub struct TradeQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub q: Option<String>,
    pub operation: Option<TradeOperation>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TradeList {
    pub items: Vec<TradeView>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TradeSummary {
    pub row_count: u64,
    pub total_debit: i64,
    pub total_credit: i64,
    pub net: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TradeView {
    pub id: String,
    pub timestamp: String,
    pub date: String,
    pub time: String,
    pub location: String,
    pub item: String,
    pub operation: TradeOperation,
    pub debit: Option<i64>,
    pub credit: Option<i64>,
}

impl TradeStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        store.initialize()?;
        Ok(store)
    }

    pub fn default_path() -> Result<PathBuf> {
        let data_dir = dirs::data_dir().ok_or_else(|| {
            DecodeError("failed to resolve local data directory for database".to_string())
        })?;
        Ok(data_dir
            .join("albion-accountant")
            .join("albion-accountant.sqlite3"))
    }

    pub fn initialize(&self) -> Result<()> {
        let connection = self.lock()?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS trades (
                id TEXT PRIMARY KEY NOT NULL,
                timestamp TEXT NOT NULL,
                location TEXT NOT NULL,
                item TEXT NOT NULL,
                operation TEXT NOT NULL CHECK(operation IN ('buy', 'sell')),
                debit INTEGER,
                credit INTEGER,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_trades_timestamp ON trades(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_trades_operation ON trades(operation);
            CREATE INDEX IF NOT EXISTS idx_trades_item ON trades(item);
            CREATE INDEX IF NOT EXISTS idx_trades_location ON trades(location);
            ",
        )?;
        Ok(())
    }

    pub fn upsert_trade(&self, trade: &TradeRecord) -> Result<()> {
        let connection = self.lock()?;
        connection.execute(
            "
            INSERT INTO trades (id, timestamp, location, item, operation, debit, credit)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
                timestamp = excluded.timestamp,
                location = excluded.location,
                item = excluded.item,
                operation = excluded.operation,
                debit = excluded.debit,
                credit = excluded.credit,
                updated_at = CURRENT_TIMESTAMP
            ",
            params![
                trade.id,
                trade.timestamp.to_rfc3339(),
                trade.location,
                trade.item,
                trade.operation_str(),
                trade.debit,
                trade.credit,
            ],
        )?;
        Ok(())
    }

    pub fn list_trades(&self, query: TradeQuery) -> Result<TradeList> {
        let page = query.page.unwrap_or(DEFAULT_PAGE).max(1);
        let page_size = query
            .page_size
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);
        let offset = (page - 1) * page_size;
        let search = query.q.as_ref().map(|value| format!("%{}%", value.trim()));
        let operation = query
            .operation
            .map(|operation| operation.as_str().to_string());
        let connection = self.lock()?;

        let total = connection.query_row(
            "
            SELECT COUNT(*)
            FROM trades
            WHERE (?1 IS NULL OR item LIKE ?1 OR location LIKE ?1 OR id LIKE ?1)
              AND (?2 IS NULL OR operation = ?2)
            ",
            params![search.as_deref(), operation.as_deref()],
            |row| row.get::<_, i64>(0),
        )? as u64;

        let mut statement = connection.prepare(
            "
            SELECT id, timestamp, location, item, operation, debit, credit
            FROM trades
            WHERE (?1 IS NULL OR item LIKE ?1 OR location LIKE ?1 OR id LIKE ?1)
              AND (?2 IS NULL OR operation = ?2)
            ORDER BY timestamp DESC, id DESC
            LIMIT ?3 OFFSET ?4
            ",
        )?;
        let rows = statement.query_map(
            params![
                search.as_deref(),
                operation.as_deref(),
                i64::from(page_size),
                i64::from(offset)
            ],
            row_to_trade_view,
        )?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }

        Ok(TradeList {
            items,
            page,
            page_size,
            total,
        })
    }

    pub fn summary(&self) -> Result<TradeSummary> {
        let connection = self.lock()?;
        let summary = connection.query_row(
            "
            SELECT COUNT(*), COALESCE(SUM(debit), 0), COALESCE(SUM(credit), 0)
            FROM trades
            ",
            [],
            |row| {
                let row_count = row.get::<_, i64>(0)? as u64;
                let total_debit = row.get::<_, i64>(1)?;
                let total_credit = row.get::<_, i64>(2)?;
                Ok(TradeSummary {
                    row_count,
                    total_debit,
                    total_credit,
                    net: total_credit - total_debit,
                })
            },
        )?;
        Ok(summary)
    }

    pub fn trade_count(&self) -> Result<u64> {
        let connection = self.lock()?;
        Ok(
            connection.query_row("SELECT COUNT(*) FROM trades", [], |row| {
                row.get::<_, i64>(0)
            })? as u64,
        )
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| DecodeError("trade database lock is poisoned".to_string()))
    }
}

fn row_to_trade_view(row: &rusqlite::Row<'_>) -> rusqlite::Result<TradeView> {
    let timestamp: String = row.get(1)?;
    let parsed_timestamp = DateTime::parse_from_rfc3339(&timestamp)
        .map(|value| value.with_timezone(&Local))
        .ok();
    let operation: String = row.get(4)?;
    let operation = TradeOperation::try_from(operation.as_str()).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
        )
    })?;

    Ok(TradeView {
        id: row.get(0)?,
        timestamp,
        date: parsed_timestamp
            .map(|timestamp| timestamp.format("%m/%d/%Y").to_string())
            .unwrap_or_default(),
        time: parsed_timestamp
            .map(|timestamp| timestamp.format("%I:%M %p").to_string())
            .unwrap_or_default(),
        location: row.get(2)?,
        item: row.get(3)?,
        operation,
        debit: row.get(5)?,
        credit: row.get(6)?,
    })
}

pub fn trade_by_id(store: &TradeStore, id: &str) -> Result<Option<TradeView>> {
    let connection = store.lock()?;
    let result = connection
        .query_row(
            "
            SELECT id, timestamp, location, item, operation, debit, credit
            FROM trades
            WHERE id = ?1
            ",
            params![id],
            row_to_trade_view,
        )
        .optional()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn upsert_is_idempotent_and_updates_values() {
        let store = memory_store();
        store
            .upsert_trade(&trade("1", TradeOperation::Buy, 100, None, "Bag"))
            .unwrap();
        store
            .upsert_trade(&trade("1", TradeOperation::Sell, None, 250, "Cape"))
            .unwrap();

        assert_eq!(store.trade_count().unwrap(), 1);
        let stored = trade_by_id(&store, "1").unwrap().unwrap();
        assert_eq!(stored.operation, TradeOperation::Sell);
        assert_eq!(stored.item, "Cape");
        assert_eq!(stored.credit, Some(250));
    }

    #[test]
    fn list_trades_paginates_and_filters() {
        let store = memory_store();
        store
            .upsert_trade(&trade("1", TradeOperation::Buy, 100, None, "Bag"))
            .unwrap();
        store
            .upsert_trade(&trade("2", TradeOperation::Sell, None, 250, "Cape"))
            .unwrap();
        store
            .upsert_trade(&trade("3", TradeOperation::Sell, None, 300, "Bag"))
            .unwrap();

        let list = store
            .list_trades(TradeQuery {
                page: Some(1),
                page_size: Some(1),
                q: Some("Bag".to_string()),
                operation: Some(TradeOperation::Sell),
            })
            .unwrap();

        assert_eq!(list.total, 1);
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].id, "3");
    }

    #[test]
    fn summary_totals_debit_credit_and_net() {
        let store = memory_store();
        store
            .upsert_trade(&trade("1", TradeOperation::Buy, 100, None, "Bag"))
            .unwrap();
        store
            .upsert_trade(&trade("2", TradeOperation::Sell, None, 250, "Cape"))
            .unwrap();

        let summary = store.summary().unwrap();

        assert_eq!(summary.row_count, 2);
        assert_eq!(summary.total_debit, 100);
        assert_eq!(summary.total_credit, 250);
        assert_eq!(summary.net, 150);
    }

    fn memory_store() -> TradeStore {
        let connection = Connection::open_in_memory().unwrap();
        let store = TradeStore {
            connection: Arc::new(Mutex::new(connection)),
        };
        store.initialize().unwrap();
        store
    }

    fn trade(
        id: &str,
        operation: TradeOperation,
        debit: impl Into<Option<i64>>,
        credit: impl Into<Option<i64>>,
        item: &str,
    ) -> TradeRecord {
        TradeRecord {
            id: id.to_string(),
            timestamp: Local.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            location: "Bridgewatch".to_string(),
            item: item.to_string(),
            operation,
            debit: debit.into(),
            credit: credit.into(),
        }
    }
}
