# Migration Note: Decode Pipeline Consolidation

The runtime decode path has been consolidated to one source-neutral engine:

```text
IngressPacket -> DecodeEngine -> TradeSemanticMapper -> MarketTransaction
```

## Removed

- Legacy `albion::decoder` packet/probe/market extraction facade.
- Legacy market mapping modules that emitted rows directly from protocol messages.
- Dormant `decode_engine::protocol18` and `decode_engine::extract` modules.
- Integration tests that parsed pcapng fixtures through retired `CapturePacket` and old extraction helpers.
- Standalone Protocol16 fixture integration tests and fixtures outside the replay parity surface.

## Why

The old code let live capture, pcap replay, transport decode, semantic mapping, and upload tests exercise different abstractions. That made parity fragile and left multiple ways to produce rows.

The new path makes pcapng replay and live capture equivalent after ingress, pins behavior with golden parity fixtures, and emits rows only from semantic trade state.

## Compatibility

Sheets upload schema did not change:

```text
Location | Item | Quantity | Per Item Cost | Total Cost
```

No decode-selection CLI flags remain. Existing capture-selection flags still apply before ingress.
