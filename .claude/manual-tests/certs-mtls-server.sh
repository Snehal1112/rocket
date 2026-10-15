#!/usr/bin/env bash
# Starts a test HTTPS server on port 9443 that demands a client certificate.
# It trusts only the fixture client certificate, so it needs no RocketVault.
# Leave it running while you test in Rocket. Stop it with Ctrl+C.
set -euo pipefail

F="/home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls"
PORT=9443

if ss -ltn | grep -q ":$PORT "; then
  echo "ERROR: port $PORT is already in use"; exit 1
fi

echo "Server on https://127.0.0.1:$PORT/ (client certificate required)."
echo "Trusted client: $F/client.pem. Press Ctrl+C to stop."
exec openssl s_server -accept "$PORT" -www \
  -cert "$F/server.pem" -key "$F/server-key.pem" \
  -Verify 1 -CAfile "$F/client.pem" -verify_return_error
