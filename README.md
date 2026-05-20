# albion-accountant

CLI tool that passively captures Albion Online market transaction traffic and appends transactions to Google Sheets.

## Linux dependencies

- `libpcap-dev` (Debian/Ubuntu)
- privileges required for packet capture (run as root or grant `cap_net_raw,cap_net_admin`)

## Safety note

This app only passively observes packets. It does **not** modify, inject into, or interfere with the Albion Online client.

## Google OAuth 2.0 (Desktop app / installed app) setup

1. In Google Cloud / Google Auth Platform, enable the **Google Sheets API**.
2. Configure the OAuth consent screen if prompted.
3. Create an **OAuth 2.0 Client ID**.
4. Choose **Desktop app** (installed app).
5. Download the client secret JSON file.
6. Save it locally:

```bash
mkdir -p ~/.config/albion-accountant
mv ~/Downloads/google-credentials.json ~/.config/albion-accountant/google-credentials.json
chmod 600 ~/.config/albion-accountant/google-credentials.json
```

7. Export environment variables:

```bash
export ALBION_ACCOUNTANT_GOOGLE_CLIENT_SECRET="$HOME/.config/albion-accountant/google-credentials.json"
export ALBION_ACCOUNTANT_GOOGLE_TOKEN_CACHE="$HOME/.config/albion-accountant/google-token-cache.json"
export ALBION_ACCOUNTANT_SPREADSHEET_ID="the_id_between_/d/_and_/edit"
export ALBION_ACCOUNTANT_SHEET_NAME="Sheet1"
export ALBION_ACCOUNTANT_INTERFACE="eth0"
```

Spreadsheet ID comes from URLs like:

`https://docs.google.com/spreadsheets/d/SPREADSHEET_ID_HERE/edit#gid=0`

Use only the `SPREADSHEET_ID_HERE` part. `gid=0` is a sheet/tab identifier, not the spreadsheet ID.

## Decoder support

The decoder supports **Photon command envelopes with Protocol16-typed payloads only**.

- Supported: Photon `Event` and `OperationResponse` messages decoded through Protocol16.
- Removed: legacy regex/JSON text fallback decoding paths.
- There are no runtime config/env decode-mode switches; decoding is protocol-only.

## Troubleshooting capture conditions

If you do not see transactions, verify these capture prerequisites:

- **IPv4/UDP visibility**: traffic must be visible as IPv4 UDP frames on the selected interface.
- **Privileges**: run with sufficient packet-capture privileges (root or `cap_net_raw,cap_net_admin`).
- **Correct interface**: choose the interface that carries Albion traffic (confirm with `--list-interfaces`).

## Usage

```bash
cargo run -- --list-interfaces
cargo run -- --dry-run --interface eth0
cargo run -- --interface eth0
```

Or explicitly pass all values:

```bash
cargo run -- \
  --interface eth0 \
  --google-client-secret "$HOME/.config/albion-accountant/google-credentials.json" \
  --google-token-cache "$HOME/.config/albion-accountant/google-token-cache.json" \
  --spreadsheet-id <spreadsheet-id> \
  --sheet-name Sheet1
```

- First non-dry-run execution opens a browser for Google authorization.
- After consent, tokens are saved to the token cache file.
- Later runs reuse cached tokens automatically.

Headers are auto-created if missing:

`Location | Item | Quantity | Per Item Cost | Total Cost`
