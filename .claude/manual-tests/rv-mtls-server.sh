#!/usr/bin/env bash
# Starts a test HTTPS server on port 9443 that demands a client certificate and
# trusts the demo certificates from the local RocketVault demo instance.
# Leave it running while you test in Rocket. Stop it with Ctrl+C.
set -euo pipefail

VAULT_DIR="/home/numericlabs/data/rocket/rv-rocket-instance"
FIXTURES="/home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls"
API="http://127.0.0.1:18443/api/v1"
PORT=9443
WORK="/tmp/rv-mtls"

echo "1/4 Checking that the RocketVault demo is running"
if ! curl -s -m 3 -o /dev/null -f "$API/health"; then
  echo "    Not running, starting it"
  (cd "$VAULT_DIR" && ./start.sh)
  sleep 3
  curl -s -m 5 -o /dev/null -f "$API/health" || { echo "    ERROR: the vault did not start"; exit 1; }
fi

echo "2/4 Fetching the public demo certificates (no private keys are saved)"
rm -rf "$WORK"; mkdir -p "$WORK"; chmod 700 "$WORK"
TOKEN=$(curl -s -u "$(cat "$VAULT_DIR/secrets/sa_client_id"):$(cat "$VAULT_DIR/secrets/sa_client_secret")" \
  -H 'Content-Type: application/x-www-form-urlencoded' --data grant_type=client_credentials \
  "$API/oauth2/token" | python3 -c "import sys,json;print(json.load(sys.stdin)['access_token'])")

fetch() { # name, certificate id
  curl -s -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{"format":"pem"}' \
    "$API/vaults/rocket-demo/certificates/$2/export" \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['certificate_pem'],end='')" > "$WORK/$1.pem"
}
fetch rsa 87d7a248-5513-4060-86ec-1b481c45a008
fetch ec  7e8132f9-011f-4723-991c-3eb7ecf003cf
cat "$WORK/rsa.pem" "$WORK/ec.pem" > "$WORK/trusted.pem"

echo "3/4 Checking that port $PORT is free"
if ss -ltn | grep -q ":$PORT "; then
  echo "    ERROR: port $PORT is already in use"; exit 1
fi

echo "4/4 Ready. Server on https://127.0.0.1:$PORT/ (client certificate required)."
echo "    Trusted: demo-rsa-exportable and demo-ec-exportable. Press Ctrl+C to stop."
exec openssl s_server -accept "$PORT" -www \
  -cert "$FIXTURES/server.pem" -key "$FIXTURES/server-key.pem" \
  -Verify 1 -CAfile "$WORK/trusted.pem" -partial_chain -verify_return_error
