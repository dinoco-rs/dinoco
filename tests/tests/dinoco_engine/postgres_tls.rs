//! PostgreSQL connections follow the url's `sslmode` the way libpq does.
//!
//! The local test server has no TLS. The scenarios against a TLS server run
//! when `DINOCO_TEST_POSTGRES_TLS_URL` points at one with a self-signed
//! certificate, for example:
//!
//! ```sh
//! openssl req -new -x509 -days 2 -nodes -subj /CN=localhost -keyout server.key -out server.crt
//! podman run -d --rm --name dinoco-pg-tls -e POSTGRES_PASSWORD=postgres -p 55432:5432 -v "$PWD":/certs:ro postgres \
//!   bash -c 'cp /certs/server.* /tmp/ && chown postgres /tmp/server.* && chmod 600 /tmp/server.key \
//!     && exec docker-entrypoint.sh postgres -c ssl=on -c ssl_cert_file=/tmp/server.crt -c ssl_key_file=/tmp/server.key'
//! DINOCO_TEST_POSTGRES_TLS_URL=postgres://postgres:postgres@localhost:55432/postgres \
//!   cargo test -p dinoco_tests --test dinoco_engine_postgres_tls
//! ```

use dinoco_engine::{PgBouncerAdapter, PostgresAdapter};

const POSTGRES_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";
const SSL_IN_USE: &str = "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()";

fn with_sslmode(url: &str, mode: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}sslmode={mode}")
}

async fn connect(url: &str) -> anyhow::Result<PostgresAdapter> {
    PostgresAdapter::direct_with_pool(url, 1, 1).await
}

async fn encrypted(url: &str) -> bool {
    let adapter = connect(url).await.unwrap_or_else(|error| panic!("{url} should connect: {error:#}"));
    adapter.query_exists(SSL_IN_USE, &[]).await.unwrap()
}

async fn connect_error(url: &str) -> String {
    match connect(url).await {
        Ok(_) => panic!("{url} should not connect"),
        Err(error) => format!("{error:#}"),
    }
}

fn tls_url() -> Option<String> {
    let url = std::env::var("DINOCO_TEST_POSTGRES_TLS_URL").ok();
    if url.is_none() {
        eprintln!("skipped: DINOCO_TEST_POSTGRES_TLS_URL is not set");
    }
    url
}

#[tokio::test]
async fn server_without_tls_connects_in_plaintext_unless_tls_is_required() {
    assert!(!encrypted(POSTGRES_URL).await, "the default `prefer` falls back to plaintext");
    assert!(!encrypted(&with_sslmode(POSTGRES_URL, "prefer")).await);
    assert!(!encrypted(&with_sslmode(POSTGRES_URL, "disable")).await);

    for mode in ["require", "verify-ca", "verify-full"] {
        let error = connect_error(&with_sslmode(POSTGRES_URL, mode)).await;
        assert!(error.contains("TLS"), "sslmode={mode}: {error}");
    }
}

#[tokio::test]
async fn unknown_sslmode_is_an_invalid_url() {
    for mode in ["allow", "verify", "VERIFY-FULL"] {
        let error = connect_error(&with_sslmode(POSTGRES_URL, mode)).await;
        assert!(error.contains("Invalid postgres url"), "sslmode={mode}: {error}");
    }
}

#[tokio::test]
async fn tls_server_is_encrypted_by_default_and_with_require() {
    let Some(url) = tls_url() else { return };

    assert!(encrypted(&url).await, "the default `prefer` uses TLS when the server has it");
    assert!(encrypted(&with_sslmode(&url, "prefer")).await);
    assert!(encrypted(&with_sslmode(&url, "require")).await, "`require` accepts a self-signed certificate");
    assert!(encrypted(&format!("{}&channel_binding=require", with_sslmode(&url, "require"))).await);
    assert!(!encrypted(&with_sslmode(&url, "disable")).await);
}

#[tokio::test]
async fn tls_server_with_untrusted_certificate_fails_verification() {
    let Some(url) = tls_url() else { return };

    for mode in ["verify-ca", "verify-full"] {
        let error = connect_error(&with_sslmode(&url, mode)).await;
        assert!(error.contains("certificate"), "sslmode={mode}: {error}");
    }
}

#[tokio::test]
async fn tls_server_runs_queries_through_pgbouncer_mode() {
    let Some(url) = tls_url() else { return };

    let adapter = PgBouncerAdapter::new(with_sslmode(&url, "require")).await.unwrap();
    assert!(adapter.query_exists(SSL_IN_USE, &[]).await.unwrap());
    assert_eq!(adapter.query_count("SELECT count(*) FROM generate_series(1, 3)", &[]).await.unwrap(), 3);
}
