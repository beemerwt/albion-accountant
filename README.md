# albion-accountant

CLI tool that passively captures Albion Online market transaction traffic and appends transactions to Google Sheets.

## Linux dependencies

- `libpcap-dev` (Debian/Ubuntu)
- privileges required for packet capture (run as root or grant `cap_net_raw,cap_net_admin`)

## Safety note

This app only passively observes packets. It does **not** modify, inject into, or interfere with the Albion Online client.

## Usage

```bash
cargo run -- --list-interfaces
cargo run -- --dry-run --interface eth0
cargo run -- \
  --interface eth0 \
  --google-credentials ./service-account.json \
  --spreadsheet-id <spreadsheet-id> \
  --sheet-name Sheet1
```

Environment fallbacks:

- `ALBION_ACCOUNTANT_INTERFACE`
- `ALBION_ACCOUNTANT_GOOGLE_CREDENTIALS`
- `ALBION_ACCOUNTANT_SPREADSHEET_ID`
- `ALBION_ACCOUNTANT_SHEET_NAME`

Headers are auto-created if missing:

`Location | Item | Quantity | Per Item Cost | Total Cost`

## Google service account setup

1. Create a service account in Google Cloud.
2. Download JSON credentials.
3. Share your target Google Sheet with the service account email as editor.
4. Provide credentials path + spreadsheet ID via CLI or env vars.
