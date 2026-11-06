# Authentication

Rocket supports the auth schemes you'll actually run into against a real API:

- **Basic**
- **Bearer**
- **API Key**
- **Digest**
- **NTLM**
- **WSSE**
- **OAuth 1.0**
- **OAuth 2.0** — client credentials, password, authorization code with PKCE,
  and implicit grants
- **AWS SigV4**
- **mTLS** client certificates
- Proxy support — see [Settings](/guide/settings) for how Rocket reaches the
  network

## Inheritance

Auth can be set on a collection, a folder, or an individual request. A request
or folder that doesn't set its own auth inherits from its parent, so a whole
collection can share one Bearer token or OAuth2 config defined at the root,
with only the exceptions configured explicitly further down the tree.

## mTLS client certificates

An environment's **Certificates** tab (inside Manage Environments) attaches a
client identity that Rocket presents to matching hosts. Three sources:

- **PEM** — a certificate and an unencrypted or passphrase-protected key file.
- **PKCS12** — a single `.p12`/`.pfx` bundle.
- **RocketVault certificate** — fetched from a connected vault at send time
  and held in memory only; it is never written to disk or saved into the
  environment file.

![The Certificates tab, with the matching rules and the three ways to add an identity](/screenshots/client-certificates.png)

Domain matching uses `*` as a wildcard (for example `*.example.com`), with an
optional `:port` to match one port specifically. **The first matching
certificate wins**, so put specific domains before wildcards. Relative PEM/PKCS12
paths resolve from the collection folder.

## Where credentials actually live

An auth scheme's *type* (OAuth2, Basic, …) is part of the collection file, but
the *secret value* backing it — a token, a client secret, a password — doesn't
have to be. See [Secrets & vault](/guide/secrets-and-vault) for the three
places a credential can actually be stored, none of which is the collection
itself.
