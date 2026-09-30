# mTLS test fixtures

Throwaway client certificate used only by the `reqwest_executor` tests. The key protects nothing.

- `client.pem` and `client-key.pem`: self-signed certificate and its PKCS#8 PEM key.
- `client.p12`: the same pair as PKCS12, password `changeit`.
- `client-key-encrypted.pem`: the key as an encrypted PKCS#8 PEM, password `changeit`.

- `server.pem` and `server-key.pem`: self-signed server certificate for the ignored handshake test.

Regenerate with:

    openssl req -x509 -newkey rsa:2048 -nodes -keyout client-key.pem -out client.pem -days 36500 -subj "/CN=rocket-test-client"
    openssl pkcs12 -export -inkey client-key.pem -in client.pem -out client.p12 -passout pass:changeit -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1
    openssl pkcs8 -topk8 -in client-key.pem -out client-key-encrypted.pem -passout pass:changeit -v2 aes-256-cbc
    openssl req -x509 -newkey rsa:2048 -nodes -keyout server-key.pem -out server.pem -days 36500 -subj "/CN=localhost" -addext "subjectAltName=IP:127.0.0.1,DNS:localhost"
