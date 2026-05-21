# Albion market protocol mapping

This document aligns the local decode pipeline with AlbionDataAvalonia's parser/handler stages so traces can be compared stage-by-stage.

## Stage mapping table

| Internal stage | AlbionDataAvalonia analogue | Notes / diagnostics emitted |
|---|---|---|
| `capture::pcap_capture` packet ingest | capture | Interface-level packet acceptance/drop counters and summary metrics in `main.rs`. |
| `decoder::extract_udp_payload` | packet parse | L2/L3/L4 parse drop reason classification (`NonUdp`, truncation, unsupported ether type). |
| `session::PacketProcessor::ingest_packet` + `transport::parse_udp_payload_incremental` | photon command framing | Per-command structured diagnostics: command kind (`reliable`, `unreliable`, `fragment`, `disconnect`, `event`, `operation_response`), channel, reliable sequence, payload length, encrypted-like prefix heuristic, plus sequence/fragment behavior summaries (duplicate suppression, queue drops, small-gap advance, large-gap resync, fragment buffering). |
| `protocol::commands::decode_command_envelope` + `decoder::probe_message` | protocol16 decode | Message-type diagnostics (`request`/`response`/`event`/`unknown`), decode success/failure probes, market opcode observations, and unsupported command-type tracking. |
| `market_mapper::{map_event_to_transaction,map_response_to_transaction}` | market mapping | Maps Protocol16 dictionaries/hashtables into `MarketTransaction` domain rows using observed operation-code/field conventions. |

## Operation codes (observed)

From `AlbionDataAvalonia.Shared/OperationCodes.cs`:

- `AuctionGetOffers` = `75` (`0x4b`)
- `AuctionGetRequests` = `76` (`0x4c`)

These are handled by:

- `AuctionGetOffersResponseHandler : base((int)OperationCodes.AuctionGetOffers)`
- `AuctionGetRequestsResponseHandler : base((int)OperationCodes.AuctionGetRequests)`

## Event codes

No event-code path is used by AlbionDataAvalonia market order upload logic. It is operation-response driven.

## Required field names

From `AlbionDataAvalonia.Network.Models/MarketOrder.cs` and response parsers:

- `LocationId` (string)
- `ItemTypeId` (string)
- `Amount` (uint)
- `UnitPriceSilver` (ulong)

The response payload parameter key used by AlbionDataAvalonia for orders is:

- param key `0` -> `IEnumerable<string>` serialized `MarketOrder` entries.

## Compatibility aliases retained in this repository

For backwards compatibility with existing fixtures in this repository, the mapper accepts these aliases only:

- `location` -> `LocationId`
- `item` -> `ItemTypeId`
- `qty` -> `Amount`
- `price` -> `UnitPriceSilver`
