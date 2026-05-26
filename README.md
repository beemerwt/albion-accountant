# albion-accountant

CLI tool that passively captures Albion Online market traffic, decodes finalized trade rows, and appends them to Google Sheets.

## Safety

This app only observes packets. It does not modify, inject into, or interfere with the Albion Online client.

## Linux Dependencies

- `libpcap-dev` on Debian/Ubuntu
- packet-capture privileges, either root or `cap_net_raw,cap_net_admin`

## Decode Model

The supported decode model is intentionally narrow and Python-equivalent:

1. Convert every source into an `IngressPacket`:
   - packet number
   - source endpoint, `ip:port`
   - destination endpoint, `ip:port`
   - UDP payload bytes
2. Feed only `IngressPacket` into `DecodeEngine`.
3. Parse Photon UDP command framing and Protocol16 typed payloads.
4. Convert decoded operation/event packets into semantic trade state.
5. Emit only finalized `MarketTransaction` rows for upload.

There is no runtime decode-mode flag and no legacy text/regex/JSON fallback decoder. Live capture and pcapng replay use the same engine after the ingress adapter boundary.

## Live Capture Path

Runtime live capture is:

```text
pcap capture backend -> live_adapter -> DecodeEngine -> TradeSemanticMapper -> SheetsClient
```

The capture filter controls which packets are observed; it does not select a decoder implementation.

## Replay Path

Replay mode parses pcapng bytes manually:

```text
pcapng file bytes -> pcapng_adapter -> DecodeEngine -> TradeSemanticMapper -> dry-run rows or SheetsClient
```

Use replay for fixture parity and deterministic local debugging:

```bash
cargo run -- --dry-run --pcap-file ./quick_buy_and_sell.pcapng
```

## Google OAuth Setup

1. In Google Cloud / Google Auth Platform, enable the Google Sheets API.
2. Configure the OAuth consent screen if prompted.
3. Create an OAuth 2.0 Client ID for a Desktop app.
4. Download the client secret JSON file.
5. Save it locally:

```bash
mkdir -p ~/.config/albion-accountant
mv ~/Downloads/google-credentials.json ~/.config/albion-accountant/google-credentials.json
chmod 600 ~/.config/albion-accountant/google-credentials.json
```

6. Copy `.env.example` to `.env` and fill in the Google Sheets values:

```bash
cp .env.example .env
```

The app loads `.env` from the repository root on startup. Environment variables already set in
your shell still take precedence.

Spreadsheet ID comes from URLs like:

```text
https://docs.google.com/spreadsheets/d/SPREADSHEET_ID_HERE/edit#gid=0
```

Use only `SPREADSHEET_ID_HERE`. `gid=0` is the sheet/tab identifier, not the spreadsheet ID.

## Usage

```bash
cargo run -- --list-interfaces
cargo run -- --dry-run --interface eth0
cargo run -- --dry-run --pcap-file ./quick_buy_and_sell.pcapng
cargo run -- --interface eth0
```

Use `--dry-run` for local capture/replay runs that should not authenticate with, create, clear, or
otherwise modify Google Sheets, even when `.env` contains Google configuration.

Or explicitly pass all Google values:

```bash
cargo run -- \
  --client-secret "$HOME/.config/albion-accountant/google-credentials.json" \
  --spreadsheet-id <spreadsheet-id> \
  --sheet-name Sheet1
```

When Google Sheets is configured, the app authenticates with OAuth, stores the token cache in
`.albion-accountant-token.json`, creates the named sheet if it is missing, and clears existing
values from that sheet.

## Tests

CI runs only the supported parity and upload-contract surface:

```bash
cargo test --test replay_parity --test sheets_contract
```

Replay parity compares manually parsed pcapng bytes against golden JSON summaries for packet statuses, message types, operation/event code names, trade state transitions, and final upload rows.

Local development can still run broader unit tests with `cargo test`, but the required compatibility gate is the parity suite above.
