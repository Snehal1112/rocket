# Manual test: client certificates in the Environment dialog

A step by step test of mTLS client certificates in the real app, with copy-paste values. Written 2026-10-15 from the source. The server commands in Part 1 were run and checked with curl (see the table there). The Rocket UI steps were not run, so the first pass is also the check of those.

Contents

- Part 0: how the feature works
- Part 1: start the test server
- Part 2: test A, a PEM certificate and key
- Part 3: test B, an encrypted key
- Part 4: test C, a PKCS12 bundle
- Part 5: test D, wrong values and error messages
- Part 6: test E, domain matching and order
- Part 7: test F, relative paths and variables
- Part 8: test G, RocketVault certificates
- Part 9: cleanup, gotchas, source locations

## Part 0: how the feature works

- Certificates live on an environment, in `<environments dir>/<name>.yml` under `clientCertificates:`.
- A certificate is sent only when the request runs with that environment selected and the URL host matches the entry's domain.
- The first matching entry in the list wins.
- Key text is never stored. An entry holds a file path, a vault secret reference, or RocketVault names.
- A plain-text passphrase is saved in the YAML as typed. Use `{{vault.NAME}}` to avoid that.
- Edits stay in the dialog until you click **Save**.

Files used below, all in `crates/rocket-infra/test-fixtures/mtls/`:

| File | What it is | Passphrase |
|---|---|---|
| `client.pem` | client certificate | none |
| `client-key.pem` | its private key, unencrypted PKCS#8 | none |
| `client-key-encrypted.pem` | same key, encrypted PKCS#8 (AES-256) | `changeit` |
| `client.p12` | PKCS12 bundle of the same cert and key | `changeit` |
| `client-key-traditional.pem` | old OpenSSL encrypted format (not supported) | `changeit` |
| `client-key-pbes1.pem` | PBES1 encrypted (not supported) | `changeit` |

Absolute path prefix used in this document:

```
/home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls
```

## Part 1: start the test server

The fixture client certificate and the fixture server certificate are unrelated self-signed certificates. The server therefore trusts the client certificate directly with `-CAfile client.pem`.

Port 9443 is used because LXD already listens on 8443 on this machine.

In a terminal:

```bash
/home/numericlabs/data/rocket/rocket/.claude/manual-tests/certs-mtls-server.sh
```

Leave it running. Stop it with Ctrl+C. The script is a wrapper around:

```bash
F=/home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls
openssl s_server -accept 9443 -www \
  -cert $F/server.pem -key $F/server-key.pem \
  -Verify 1 -CAfile $F/client.pem -verify_return_error
```

Check the server without Rocket, in a second terminal. These were run on 2026-10-15:

```bash
F=/home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls
# Client cert and unencrypted key: expect 200
curl -sk --cert $F/client.pem --key $F/client-key.pem https://127.0.0.1:9443/ -o /dev/null -w "%{http_code}\n"
# Encrypted key: expect 200
curl -sk --cert $F/client.pem --key $F/client-key-encrypted.pem --pass changeit https://127.0.0.1:9443/ -o /dev/null -w "%{http_code}\n"
# PKCS12: expect 200
curl -sk --cert-type P12 --cert $F/client.p12:changeit https://127.0.0.1:9443/ -o /dev/null -w "%{http_code}\n"
# No client cert: expect 000 (the handshake is refused)
curl -sk https://127.0.0.1:9443/ -o /dev/null -w "%{http_code}\n"
```

| Check | Result on 2026-10-15 |
|---|---|
| PEM cert and key | 200 |
| Encrypted key, `changeit` | 200 |
| PKCS12, `changeit` | 200 |
| No client cert | 000 |

The server terminal prints almost nothing. The response in Rocket is your signal.

## Part 2: test A, a PEM certificate and key (the main test)

Goal: prove that a request fails without the certificate and succeeds with it.

### A1. The request fails first

1. Start Rocket: `yarn tauri dev`.
2. Create a request.
   - Method: GET
   - URL: `https://127.0.0.1:9443/`
3. Turn **Verify SSL certificate** off. The server certificate is self-signed. With verify on, the request fails with only `error sending request`.
4. Send.

Expected: an error such as `HTTP error: error sending request for url (https://127.0.0.1:9443/)`. This is the baseline. The server refuses the handshake because no client certificate was sent.

### A2. Add the certificate

1. Open the environment switcher and click **Configure**. The dialog is titled **Manage Environments**.
2. In the left list, select an environment. Create one first if the list is empty. Name it `mtls-test`.
3. Open the **Certificates** tab. It is the third tab, after **Variables** and **External Secrets**.
4. Click **Add PEM**. A row appears with a **PEM** badge.
5. Fill the row:

| Field | Value |
|---|---|
| Domain | `127.0.0.1` |
| Certificate, source | File |
| Certificate, path | `/home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls/client.pem` |
| Private key, source | File |
| Private key, path | `/home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls/client-key.pem` |
| Passphrase | leave empty |

You can type the path or click **Browse** and pick the file. The picker has no extension filter.

6. Click **Save**. The button shows a spinner and then "Saved".

### A3. Send again with the environment selected

1. Close the dialog.
2. Select `mtls-test` in the environment switcher. A certificate in an environment that is not selected is ignored.
3. Send the same request.

Expected: a normal response. `openssl s_server -www` returns an HTML status page with the cipher and session details. If you see it, the client certificate was presented and accepted.

### A4. Prove the certificate was the cause

Do both checks, then restore the setup.

- Select a different environment (or none) and send. Expected: the baseline error from A1 again.
- Change the domain to `localhost`, **Save**, and send with `mtls-test` selected. Expected: the baseline error. The domain no longer matches `127.0.0.1`, so the certificate is silently skipped.

Change the domain back to `127.0.0.1` and **Save** before the next test.

### A5. Check the saved file

The environment is saved as YAML. For a collection environment the file is `<collection>/environments/mtls-test.yml`. For a global environment it is in the active workspace's `environments/` folder under `~/.rocket-api`.

Expected shape:

```yaml
name: mtls-test
clientCertificates:
  - type: pem
    domain: 127.0.0.1
    certificateFilePath: /home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls/client.pem
    privateKeyFilePath: /home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls/client-key.pem
```

There must be no key text in the file, only paths.

## Part 3: test B, an encrypted private key

1. In the Certificates tab, edit the PEM row from test A.
2. Change the **Private key** path to `.../client-key-encrypted.pem`.
3. Leave **Passphrase** empty and click **Save**. A passphrase left empty is allowed at save time.
4. Select `mtls-test` and send.

Expected error: `Invalid input: The private key <path> is encrypted. Enter its passphrase.`

5. Enter the passphrase `changeit`.
6. **Save**. A warning toast appears: "the passphrase is a literal and will be saved in the environment file. Use a vault secret placeholder such as {{vault.NAME}}." An amber note shows under the field too. Saving still works.
7. Send again.

Expected: a normal response.

Wrong passphrase variant: set the passphrase to `wrong`, **Save**, send.

Expected error: `Invalid input: Cannot decrypt the private key <path>: wrong passphrase or damaged file.`

Placeholder variant (optional, to avoid the warning): create an environment variable or a vault secret named for the passphrase and set **Passphrase** to `{{vault.NAME}}`, or use the **Insert vault secret** dropdown next to the field. This needs an External Secrets binding with fetched secrets.

Give passphrases exactly as shown. A trailing `.` copied from prose makes it a wrong passphrase.

## Part 4: test C, a PKCS12 bundle

1. Remove the PEM row with the X button, or leave it and add a second row. For a clean test, remove it.
2. Click **Add PKCS12**.
3. Fill the row:

| Field | Value |
|---|---|
| Domain | `127.0.0.1` |
| PKCS12 bundle, source | File |
| PKCS12 bundle, path | `/home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls/client.p12` |
| Passphrase | `changeit` |

4. **Save** and send with `mtls-test` selected.

Expected: a normal response.

Wrong passphrase variant: set `wrong`, **Save**, send.

Expected error: `Invalid input: Cannot load PKCS12 client certificate <path>: builder error. Check the file and its passphrase.` The words `builder error` are all you get. A wrong passphrase and an unsupported bundle look the same.

## Part 5: test D, wrong values and error messages

These show the messages you see when you click **Save**. Only the first error is shown, as a toast, and the dialog switches to the Certificates tab. Messages are prefixed `Certificate N (domain):`.

| What you do | Expected message |
|---|---|
| Add PEM, leave Domain empty, **Save** | `Certificate 1: domain is required.` |
| Domain set, both paths empty | `Certificate 1 (127.0.0.1): Certificate needs a file path or a vault secret.` |
| Paste `-----BEGIN CERTIFICATE-----` into a path field | `... file path must be a file path or a vault secret reference, not key text.` |
| Relative path `../x/client.pem` | `... file path must not contain "..".` |
| Source set to Vault secret with nothing fetched | `... is not one of the fetched secrets ... Fetch the secrets on the External Secrets tab.` |

These errors appear at send time, not at save time:

| What you do | Expected message |
|---|---|
| Path to a file that does not exist | `Invalid input: Cannot read client certificate file <path>: <io error>` |
| Key `client-key-traditional.pem` | `Invalid input: The private key <path> uses the old OpenSSL encrypted format, which is not supported. Convert it with openssl pkcs8 -topk8 -v2 aes-256-cbc ...` |
| Key `client-key-pbes1.pem` | `Invalid input: ... uses an old encryption scheme (PBES1), which is not supported. ...` |
| Certificate file is not a certificate | `Invalid input: Cannot load PEM client certificate <path>: builder error` |

A failed save does not change the saved file, and an invalid certificate also blocks saving variables, because Save writes the whole environment.

## Part 6: test E, domain matching and order

Matching rules, from the code and its unit tests:

- `*` is a wildcard. `*.example.com` matches `api.example.com` but not `example.com`.
- A domain with a port, such as `127.0.0.1:9443`, matches only that port. Without a port it matches any port.
- A scheme or path in the domain is stripped, so `https://127.0.0.1` works the same as `127.0.0.1`.
- An empty domain never matches.
- The first matching entry in the list wins.

Try this, using one environment with two PEM rows that both point at the fixtures:

1. Row 1: domain `*`, certificate `client.pem`, key `client-key-encrypted.pem`, **no passphrase**.
2. Row 2: domain `127.0.0.1:9443`, certificate `client.pem`, key `client-key.pem`.
3. **Save**, select the environment, send to `https://127.0.0.1:9443/`.

Expected: the request fails with `The private key ... is encrypted. Enter its passphrase.`, because row 1 matches first.

4. Click the down arrow on row 1 (aria label "Move certificate 1 down"), so the specific row is first. **Save** and send again.

Expected: a normal response. Row 2 now wins.

Other domain checks, with `https://127.0.0.1:9443/` as the URL:

| Domain | Expected |
|---|---|
| `127.0.0.1` | sent |
| `127.0.0.1:9443` | sent |
| `127.0.0.1:9444` | not sent, request fails |
| `*.0.0.1` | sent (wildcard) |
| `localhost` | not sent, request fails |

## Part 7: test F, relative paths and variables

### Relative path

A relative path starts at the collection folder. It must not contain `..`.

1. Copy the fixtures into your collection folder, in a `certs/` subfolder:

```bash
mkdir -p <collection>/certs
cp /home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls/client.pem \
   /home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls/client-key.pem \
   <collection>/certs/
```

2. Use a collection environment, not a global one. A global environment has no collection folder to resolve against.
3. In the Certificates tab set the certificate path to `certs/client.pem` and the key path to `certs/client-key.pem`. Domain `127.0.0.1`.
4. **Save** and send.

Expected: a normal response. A `../` path is refused at save time.

### `~` path

Copy the files to `~/rocket-certs/` and use `~/rocket-certs/client.pem`. Expected: a normal response.

### Variables in the domain and the paths

1. On the **Variables** tab add `mtlsHost` = `127.0.0.1` and `certDir` = `/home/numericlabs/data/rocket/rocket/crates/rocket-infra/test-fixtures/mtls`.
2. On the Certificates tab set:
   - Domain: `{{mtlsHost}}`
   - Certificate path: `{{certDir}}/client.pem`
   - Private key path: `{{certDir}}/client-key.pem`
3. **Save** and send.

Expected: a normal response. Variables in the domain, paths and passphrase are resolved before matching.

## Part 8: test G, RocketVault certificates (optional)

Only needed to test the vault path. It uses the demo vault, not the fixtures.

1. Stop the fixture server from Part 1. Both servers use port 9443.
2. Start the vault server script:

```bash
/home/numericlabs/data/rocket/rocket/.claude/manual-tests/rv-mtls-server.sh
```

It starts the RocketVault demo if needed (`http://127.0.0.1:18443`, vault `rocket-demo`, service account `rocket-exporter`) and trusts the demo certificates `demo-rsa-exportable` and `demo-ec-exportable`.

3. In **Manage Environments**, open **External Secrets** and add a binding to the RocketVault connection. The service account needs the role "Key Vault Secrets User" for the connection test to list secrets. Fetch the secrets.
4. Open **Certificates**. **Add RocketVault certificate** is visible only once a binding to RocketVault exists.
5. Click it and fill the row:

| Field | Value |
|---|---|
| Domain | `127.0.0.1` |
| Binding | the alias you created |
| Certificate | `demo-rsa-exportable` |
| Format | PEM |

Entries that are not exportable or are disabled show greyed out and cannot be picked. Use the reload icon after changes in the vault.

6. **Save**, select the environment, and send to `https://127.0.0.1:9443/` with **Verify SSL certificate** off.

Expected: a normal response. The certificate is exported at send time and never saved. The saved YAML holds only names:

```yaml
clientCertificates:
  - type: vault
    domain: 127.0.0.1
    binding: demo
    certificate: demo-rsa-exportable
    format: pem
```

Other vault checks:

- Try `demo-ec-exportable` with format PEM. On Windows the row shows an EC key warning. On Linux it should work.
- Try format PKCS12. This asks the vault for a legacy-compat bundle, which this machine's TLS library may not load. If it fails you get `builder error`. PEM avoids this. Record the result, because it is untested.
- Delete the binding in the External Secrets tab, then look at the certificate row. It shows "Binding <b> is not in this environment. Pick one from the list."

## Part 9: cleanup, gotchas, source locations

Cleanup:

- Stop the server with Ctrl+C.
- Delete the `mtls-test` environment, or remove the certificate rows.
- Remove `<collection>/certs/` and `~/rocket-certs/` if you made them.
- Nothing from these tests is committed. The fixtures are not modified.

Gotchas:

- Verify SSL on against the self-signed server fails with only `error sending request`. The real TLS cause is not shown.
- Verify SSL off does not skip the client certificate. The certificate is still sent.
- A non-matching domain is silently skipped. There is no log line saying a certificate was presented.
- Collection-level `clientCertificates` in `opencollection.yml` are parsed but not applied. Only an environment's certificates are used.
- The UI has no way to paste PEM text. Use a file path or a vault secret.
- There is no per-certificate enable toggle and no "test connection" button for certificates.
- A redirect that leaves the certificate's domain is not followed. The 3xx is returned.
- Certificates also apply to OAuth2 token URLs that match the domain.
- `.claude/rocketvault-certificate-export-requirements.md` has stale status lines. The `vault` certificate type already exists.

Key source locations:

- UI: `src/components/environments/CertificatesTab.tsx`, `VaultCertificateRow.tsx`, `EnvironmentDialog.tsx`
- Frontend validation: `src/lib/certificate-validation.ts`
- Matching: `crates/rocket-http/src/client_cert.rs`
- Identity loading and error strings: `crates/rocket-http/src/reqwest_executor.rs`, `crates/rocket-infra/src/pem_key.rs`
- Persistence model: `crates/rocket-shared/src/certificate.rs`
