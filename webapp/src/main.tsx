import React, { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';

type Operation = 'buy' | 'sell';

type Trade = {
  id: string;
  timestamp: string;
  date: string;
  time: string;
  location: string;
  item: string;
  operation: Operation;
  debit: number | null;
  credit: number | null;
};

type TradeList = {
  items: Trade[];
  page: number;
  page_size: number;
  total: number;
};

type Summary = {
  row_count: number;
  total_debit: number;
  total_credit: number;
  net: number;
};

const pageSize = 25;

function App() {
  const [trades, setTrades] = useState<TradeList | null>(null);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [page, setPage] = useState(1);
  const [query, setQuery] = useState('');
  const [operation, setOperation] = useState('');
  const [refreshKey, setRefreshKey] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const totalPages = useMemo(() => {
    if (!trades) return 1;
    return Math.max(1, Math.ceil(trades.total / trades.page_size));
  }, [trades]);

  useEffect(() => {
    const controller = new AbortController();
    const params = new URLSearchParams({
      page: page.toString(),
      page_size: pageSize.toString()
    });
    if (query.trim()) params.set('q', query.trim());
    if (operation) params.set('operation', operation);

    setLoading(true);
    setError(null);

    Promise.all([
      fetch(`/api/trades?${params}`, { signal: controller.signal }).then(expectJson<TradeList>),
      fetch('/api/summary', { signal: controller.signal }).then(expectJson<Summary>)
    ])
      .then(([tradeList, tradeSummary]) => {
        setTrades(tradeList);
        setSummary(tradeSummary);
      })
      .catch((err: unknown) => {
        if ((err as Error).name !== 'AbortError') {
          setError((err as Error).message || 'Failed to load trades');
        }
      })
      .finally(() => setLoading(false));

    return () => controller.abort();
  }, [page, query, operation, refreshKey]);

  function updateQuery(value: string) {
    setQuery(value);
    setPage(1);
  }

  function updateOperation(value: string) {
    setOperation(value);
    setPage(1);
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <h1>Albion Accountant</h1>
          <p>Local trade ledger</p>
        </div>
        <button className="refresh-button" type="button" onClick={() => setRefreshKey((value) => value + 1)}>
          Refresh
        </button>
      </header>

      <section className="summary-grid" aria-label="Trade totals">
        <SummaryTile label="Rows" value={summary?.row_count ?? 0} />
        <SummaryTile label="Debit" value={formatSilver(summary?.total_debit ?? 0)} />
        <SummaryTile label="Credit" value={formatSilver(summary?.total_credit ?? 0)} />
        <SummaryTile label="Net" value={formatSilver(summary?.net ?? 0)} tone={(summary?.net ?? 0) >= 0 ? 'good' : 'bad'} />
      </section>

      <section className="toolbar" aria-label="Trade filters">
        <input
          value={query}
          onChange={(event) => updateQuery(event.target.value)}
          placeholder="Search item, location, or ID"
        />
        <select className="dropdown operation-filter" value={operation} onChange={(event) => updateOperation(event.target.value)}>
          <option value="">All operations</option>
          <option value="buy">Buys</option>
          <option value="sell">Sells</option>
        </select>
      </section>

      {error ? <div className="notice error">{error}</div> : null}
      {loading ? <div className="notice">Loading trades...</div> : null}

      <section className="table-wrap" aria-label="Trades">
        <table>
          <thead>
            <tr>
              <th>Date</th>
              <th>Time</th>
              <th>Location</th>
              <th>Item</th>
              <th>Operation</th>
              <th className="numeric">Debit</th>
              <th className="numeric">Credit</th>
            </tr>
          </thead>
          <tbody>
            {trades?.items.map((trade) => (
              <tr key={trade.id}>
                <td>{trade.date}</td>
                <td>{trade.time}</td>
                <td>{trade.location}</td>
                <td>{trade.item}</td>
                <td><span className={`op ${trade.operation}`}>{trade.operation}</span></td>
                <td className="numeric">{formatOptionalSilver(trade.debit)}</td>
                <td className="numeric">{formatOptionalSilver(trade.credit)}</td>
              </tr>
            ))}
            {!loading && trades?.items.length === 0 ? (
              <tr>
                <td colSpan={7} className="empty">No trades match the current filters.</td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>

      <footer className="pager">
        <button type="button" disabled={page <= 1} onClick={() => setPage((value) => Math.max(1, value - 1))}>
          Previous
        </button>
        <span>Page {page} of {totalPages}</span>
        <button type="button" disabled={page >= totalPages} onClick={() => setPage((value) => value + 1)}>
          Next
        </button>
      </footer>
    </main>
  );
}

function SummaryTile({ label, value, tone }: { label: string; value: string | number; tone?: 'good' | 'bad' }) {
  return (
    <div className={`summary-tile ${tone ?? ''}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

async function expectJson<T>(response: Response): Promise<T> {
  if (!response.ok) {
    throw new Error(await response.text());
  }
  return response.json() as Promise<T>;
}

function formatOptionalSilver(value: number | null) {
  return value == null ? '' : formatSilver(value);
}

function formatSilver(value: number) {
  return new Intl.NumberFormat().format(value);
}

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
