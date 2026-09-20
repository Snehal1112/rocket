//! Local, no-network integration fixtures for the fail-closed certificate
//! verification path. Unlike `ssh_host_verification`'s tests (which classify
//! against synthetic known_hosts files without a live connection), these
//! spin up a real local TLS listener presenting a self-signed certificate so
//! a genuine `git2` clone attempt exercises the actual certificate_check
//! callback wiring end to end.

use std::net::TcpListener;
use std::thread;

use openssl::asn1::Asn1Time;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::ssl::{SslAcceptor, SslMethod};
use openssl::x509::{X509NameBuilder, X509};
use rocket_shared::error::DomainError;
use tempfile::TempDir;

use crate::credentials::GitCredentials;
use crate::service::GitService;

use super::Git2Service;

fn self_signed_certificate() -> (X509, PKey<openssl::pkey::Private>) {
    let rsa = Rsa::generate(2048).expect("RSA key generation should succeed");
    let pkey = PKey::from_rsa(rsa).expect("wrapping RSA key in PKey should succeed");

    let mut name_builder = X509NameBuilder::new().expect("X509 name builder should be created");
    name_builder
        .append_entry_by_text("CN", "127.0.0.1")
        .expect("common name should be set");
    let name = name_builder.build();

    let mut builder = X509::builder().expect("X509 builder should be created");
    builder.set_version(2).expect("version should be set");
    builder
        .set_subject_name(&name)
        .expect("subject name should be set");
    builder
        .set_issuer_name(&name)
        .expect("issuer name should be set");
    builder.set_pubkey(&pkey).expect("public key should be set");
    builder
        .set_not_before(&Asn1Time::days_from_now(0).expect("not-before should be computed"))
        .expect("not-before should be set");
    builder
        .set_not_after(&Asn1Time::days_from_now(1).expect("not-after should be computed"))
        .expect("not-after should be set");
    builder
        .sign(&pkey, MessageDigest::sha256())
        .expect("self-signing should succeed");

    (builder.build(), pkey)
}

/// Regression fixture for the fail-closed TLS path: a local HTTPS server
/// presenting a self-signed certificate must cause a real `clone_repo` call
/// to fail with a typed `TlsCertificateInvalid` error, never a silent
/// success and never an unclassified `Internal` error.
#[test]
fn clone_over_https_with_self_signed_certificate_fails_closed_with_typed_error() {
    let (cert, pkey) = self_signed_certificate();

    let listener =
        TcpListener::bind("127.0.0.1:0").expect("listener should bind an ephemeral port");
    let port = listener
        .local_addr()
        .expect("listener should have a local address")
        .port();

    let server = thread::spawn(move || {
        let mut acceptor_builder = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls())
            .expect("SSL acceptor builder should be created");
        acceptor_builder
            .set_private_key(&pkey)
            .expect("private key should be set on the acceptor");
        acceptor_builder
            .set_certificate(&cert)
            .expect("certificate should be set on the acceptor");
        let acceptor = acceptor_builder.build();

        if let Ok((stream, _)) = listener.accept() {
            // The client (git2/libgit2) is expected to reject this
            // self-signed certificate during the handshake itself; any
            // handshake error on the server side is expected and ignored.
            let _ = acceptor.accept(stream);
        }
    });

    let dest = TempDir::new().expect("temp destination dir should be created");
    let dest_path = dest.path().join("clone");
    let url = format!("https://127.0.0.1:{port}/repo.git");

    let result = Git2Service::new().clone_repo(
        &url,
        &dest_path.to_string_lossy(),
        &GitCredentials::UserPass {
            username: String::new(),
            password: String::new(),
        },
    );

    assert!(
        matches!(
            &result,
            Err(DomainError::TlsCertificateInvalid { host, port: p, .. })
                if host == "127.0.0.1" && *p == port
        ),
        "expected a typed TlsCertificateInvalid error for 127.0.0.1:{port}, got: {result:?}"
    );

    server.join().expect("server thread should not panic");
}
