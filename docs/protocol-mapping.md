# Protocol Mapping

This document describes the supported decode model after the legacy decoder removal.

## Python-Equivalent Model

The Rust runtime mirrors the Python replay model at the ingress boundary:

```text
source bytes -> IngressPacket -> DecodeEngine -> DecodedPacket -> TradeSemanticMapper -> MarketTransaction
```

`IngressPacket` is the only input record the decode layer accepts. It contains packet number, source endpoint, destination endpoint, and UDP payload bytes. This keeps live capture and pcapng replay behavior equivalent after packet ingress.

## Stage Mapping

| Stage | Rust module | Responsibility | Parity assertion |
|---|---|---|---|
| Source ingress | `live_adapter`, `pcapng_adapter` | Convert live packets or pcapng bytes into `IngressPacket`. | Ingress packet count and non-empty UDP payloads. |
| Photon framing | `albion::session`, `albion::protocol::transport` | Parse Photon UDP command packets, handle incomplete fragments, sequence gaps, duplicate suppression, and command diagnostics. | Packet status, command kind, and message-type histograms. |
| Command envelope | `albion::protocol::commands` | Normalize command body into `PhotonMessage` metadata and payload bytes. | Message type and command diagnostic counts. |
| Protocol16 decode | `albion::protocol::{events,operations,protocol16}` | Decode event and operation payload maps. | Operation/event code IDs and generated enum names. |
| Semantic mapping | `trade_mapping::semantics` | Cache listing orders, stage buy/sell requests, confirm or clear pending trades on responses, and map market notification events when they contain full row data. | Golden trade state transitions and final transaction rows. |
| Upload contract | `sheets::row`, `sheets::client` | Preserve the five-column row schema and append finalized rows. | Minimal Sheets row contract and mocked uploader boundary. |

## Live Capture Path

```text
pcap backend -> live_adapter -> DecodeEngine -> TradeSemanticMapper -> SheetsClient
```

Capture filter flags only affect packet selection before `live_adapter`. They do not select a decode implementation.

## Pcapng Fixture Parity

Fixture tests read `.pcapng` files directly with `fs::read` and pass bytes into `pcapng_adapter::parse_pcapng`. The `pcap` capture library is not involved in fixture decode tests.

Parity tests compare JSON golden files under `tests/fixtures`:

- `quick_buy_and_sell.decoded_summary.expected.json`: packet statuses, command/message types, decoded packet counts, operation/event code names, and emitted transaction rows for a real capture.
- `semantic_trade_flow.expected.json`: synthetic decoded packet stream covering cached orders, staged requests, confirmed/cleared responses, market notification rows, and final upload rows.

## Operation And Event Names

Operation and event names are generated from the Rust enum source files:

- `src/albion/operation_codes.rs`
- `src/albion/event_codes.rs`

The parity suite asserts both numeric codes and generated names so enum drift is visible in golden diffs.
