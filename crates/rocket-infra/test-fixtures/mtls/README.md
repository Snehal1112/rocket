# mTLS test fixtures

Throwaway client certificate used only by the `reqwest_executor` tests. The key protects nothing.

- `client.pem` and `client-key.pem`: self-signed certificate and its PKCS#8 PEM key.
- `client.p12`: the same pair as PKCS12, password `changeit`.
- `client-key-encrypted.pem`: the key as an encrypted PKCS#8 PEM, password `changeit`.
- `client-key-scrypt.pem`: the key as an encrypted PKCS#8 PEM with scrypt KDF, password `changeit`.
- `client-key-3des.pem`: the key as an encrypted PKCS#8 PEM with 3DES encryption, password `changeit`.
- `client-key-sha1prf.pem`: the key as an encrypted PKCS#8 PEM with HMAC-SHA1 PRF, password `changeit`.
- `client-key-pbes1.pem`: the key as an encrypted PKCS#8 PEM with PBES1, password `changeit`.
- `client-key-traditional.pem`: the key as a traditional encrypted RSA PEM, password `changeit`.

- `server.pem` and `server-key.pem`: self-signed server certificate for the ignored handshake test.

Regenerate with:

    openssl req -x509 -newkey rsa:2048 -nodes -keyout client-key.pem -out client.pem -days 36500 -subj "/CN=rocket-test-client"
    openssl pkcs12 -export -inkey client-key.pem -in client.pem -out client.p12 -passout pass:changeit -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1
    openssl pkcs8 -topk8 -in client-key.pem -out client-key-encrypted.pem -passout pass:changeit -v2 aes-256-cbc
    openssl pkcs8 -topk8 -in client-key.pem -out client-key-scrypt.pem -v2 aes-256-cbc -scrypt -passout pass:changeit
    openssl pkcs8 -topk8 -in client-key.pem -out client-key-3des.pem -v2 des3 -passout pass:changeit
    openssl pkcs8 -topk8 -in client-key.pem -out client-key-sha1prf.pem -v2 aes-256-cbc -v2prf hmacWithSHA1 -passout pass:changeit
    openssl pkcs8 -topk8 -in client-key.pem -out client-key-pbes1.pem -v1 PBE-SHA1-3DES -passout pass:changeit
    openssl rsa -in client-key.pem -traditional -aes256 -passout pass:changeit -out client-key-traditional.pem
    openssl req -x509 -newkey rsa:2048 -nodes -keyout server-key.pem -out server.pem -days 36500 -subj "/CN=localhost" -addext "subjectAltName=IP:127.0.0.1,DNS:localhost"
