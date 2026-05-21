export ALBION_ACCOUNTANT_GOOGLE_CLIENT_SECRET="./google-credentials.json"
export ALBION_ACCOUNTANT_SPREADSHEET_ID="1Gf9NkecYp17iCqoTQt_o-_uPbnSC8-O2XadAXskMFvE"
export ALBION_ACCOUNTANT_SHEET_NAME="Market Data"
export RUST_LOG=debug

cargo build
sudo setcap cap_net_raw,cap_net_admin=eip ./target/debug/albion-accountant
./target/debug/albion-accountant --interface enp41s0 --dry-run