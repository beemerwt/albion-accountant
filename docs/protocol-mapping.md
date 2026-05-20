# Albion market protocol mapping

This project now maps market transactions to the same **observed** operation codes and field names used by AlbionDataAvalonia.

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
