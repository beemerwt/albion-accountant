# albion-accountant

Local-first tool that passively captures Albion Online market traffic, stores finalized trade rows in SQLite, serves a browser UI, and can optionally append them to Google Sheets.

## Safety

This app only observes packets. It does not modify, inject into, or interfere with the Albion Online client.

## Linux Dependencies

- `libpcap-dev` on Debian/Ubuntu
- packet-capture privileges, either root or `cap_net_raw,cap_net_admin`
- tray icon support on Debian/Ubuntu:
  `sudo apt install libgtk-3-dev libayatana-appindicator3-dev`
  or use `libappindicator3-dev` instead of `libayatana-appindicator3-dev`

On Ubuntu GNOME Wayland, the tray icon also needs AppIndicator/StatusNotifier support enabled.
Ubuntu commonly provides this through its AppIndicator GNOME Shell extension.

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
pcap capture backend -> live_adapter -> DecodeEngine -> TradeSemanticMapper -> SQLite -> Web UI
```

The capture filter controls which packets are observed; it does not select a decoder implementation.
If Google Sheets is configured, the app also syncs stored trade rows to the configured sheet.

## Replay Path

Replay mode parses pcapng bytes manually:

```text
pcapng file bytes -> pcapng_adapter -> DecodeEngine -> TradeSemanticMapper -> SQLite -> optional SheetsClient
```

Use replay only for fixture parity and deterministic local debugging:

```bash
cargo run -- --dry-run --pcap-file ./quick_buy_and_sell.pcapng
```

## Local Web UI

Live capture starts a localhost webserver on a random available `127.0.0.1` port. Double-click the
tray icon to open the web app. The app shows locally stored trades with pagination, search,
operation filtering, and debit/credit/net totals.

The default database path is the platform user data directory, such as:

```text
~/.local/share/albion-accountant/albion-accountant.sqlite3
```

Override it with `--database-path` or `ALBION_ACCOUNTANT_DATABASE_PATH`.

Build the React app for embedding with:

```bash
cd webapp
npm install
npm run build
```

If `webapp/dist` does not exist, the Rust binary serves a small fallback page explaining how to
build the React UI.

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
cargo run
cargo run -- --dry-run
cargo run -- --dry-run --pcap-file ./quick_buy_and_sell.pcapng
```

Use `--dry-run` for local capture/replay runs that should not authenticate with, create, clear, or
otherwise modify Google Sheets, even when `.env` contains Google configuration. Local SQLite
storage still records trades.

Live capture on Linux starts immediately and adds an Albion Accountant tray icon. Use the tray menu
to stop/start capture or exit the app. Double-click the tray icon to open the web UI. Replay mode
remains CLI-only.

Or explicitly pass all Google values:

```bash
cargo run -- \
  --client-secret "$HOME/.config/albion-accountant/google-credentials.json" \
  --spreadsheet-id <spreadsheet-id> \
  --sheet-name Sheet1
```

When Google Sheets is configured and `--dry-run` is not used, the app authenticates with OAuth, stores the token cache in
`.albion-accountant-token.json`, creates the named sheet if it is missing, verifies the header, and
syncs decoded trades after local SQLite persistence. Existing matching sheets are not cleared.

## Tests

CI runs only the supported parity and upload-contract surface:

```bash
cargo test --test replay_parity --test sheets_contract
```

Replay parity compares manually parsed pcapng bytes against golden JSON summaries for packet statuses, message types, operation/event code names, trade state transitions, and final upload rows.

Local development can still run broader unit tests with `cargo test`, but the required compatibility gate is the parity suite above.
