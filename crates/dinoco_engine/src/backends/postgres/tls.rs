//! TLS for PostgreSQL connections, following the url's `sslmode` the way libpq does.

use std::str::FromStr;
use std::sync::Arc;

use anyhow::Context;
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio_postgres::Config;
use tokio_postgres::config::SslMode;
use tokio_postgres_rustls::MakeRustlsConnect;

/// How a connection is encrypted, with the meaning libpq gives each `sslmode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TlsMode {
    /// `disable`: plaintext.
    Disable,
    /// `prefer` (the default) and `require`: encrypted, without checking the
    /// server certificate. `prefer` stays plaintext when the server has no TLS.
    Encrypt,
    /// `verify-ca`: the certificate must chain to a trusted root.
    VerifyCa,
    /// `verify-full`: `verify-ca`, and the certificate must be issued for the host.
    VerifyFull,
}

/// Parses a PostgreSQL url and its TLS mode. `tokio-postgres` only knows
/// `disable`, `prefer` and `require`, so `verify-ca` and `verify-full` are read
/// here and handed to it as `require`.
pub(super) fn parse_url(url: &str) -> anyhow::Result<(Config, TlsMode)> {
    let verify = [("sslmode=verify-full", TlsMode::VerifyFull), ("sslmode=verify-ca", TlsMode::VerifyCa)]
        .into_iter()
        .find_map(|(param, mode)| find_param(url, param).map(|at| (at..at + param.len(), mode)));
    let config = match &verify {
        Some((range, _)) => {
            let mut url = url.to_owned();
            url.replace_range(range.clone(), "sslmode=require");
            Config::from_str(&url)
        }
        None => Config::from_str(url),
    }
    .context("Invalid postgres url")?;

    let mode = match verify {
        Some((_, mode)) => mode,
        None if config.get_ssl_mode() == SslMode::Disable => TlsMode::Disable,
        None => TlsMode::Encrypt,
    };

    Ok((config, mode))
}

/// Byte offset of `param` as a whole `key=value` pair, either in a url's query
/// string or among the whitespace-separated pairs of a key/value string.
fn find_param(url: &str, param: &str) -> Option<usize> {
    url.match_indices(param).map(|(at, _)| at).find(|&at| {
        let before = url[..at].chars().next_back();
        let after = url[at + param.len()..].chars().next();
        before.is_none_or(|c| c == '?' || c == '&' || c.is_whitespace())
            && after.is_none_or(|c| c == '&' || c.is_whitespace())
    })
}

/// The TLS connector for `mode`, or `None` for `disable`.
///
/// Uses the process's default crypto provider when the application installed
/// one, `ring` otherwise, so it never panics when both `ring` and `aws-lc-rs`
/// are compiled into the binary. Trusted roots are Mozilla's, embedded through
/// `webpki-roots`, so no system certificate store is needed.
pub(super) fn connector(mode: TlsMode) -> anyhow::Result<Option<MakeRustlsConnect>> {
    if mode == TlsMode::Disable {
        return Ok(None);
    }

    let provider = match CryptoProvider::get_default() {
        Some(provider) => Arc::clone(provider),
        None => Arc::new(rustls::crypto::ring::default_provider()),
    };
    let verifier: Arc<dyn ServerCertVerifier> = match mode {
        TlsMode::Disable => unreachable!(),
        TlsMode::Encrypt => Arc::new(AnyCertificate(provider.signature_verification_algorithms)),
        TlsMode::VerifyCa | TlsMode::VerifyFull => {
            let roots = Arc::new(RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() });
            let webpki = WebPkiServerVerifier::builder_with_provider(roots, Arc::clone(&provider))
                .build()
                .context("Failed to configure TLS certificate verification for postgres")?;
            match mode {
                TlsMode::VerifyCa => Arc::new(AnyHostname(webpki)),
                _ => webpki,
            }
        }
    };

    let mut config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("Failed to configure TLS for postgres")?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    // Required by `sslnegotiation=direct` (PostgreSQL 17+); libpq sends it on every TLS connection.
    config.alpn_protocols = vec![b"postgresql".to_vec()];

    Ok(Some(MakeRustlsConnect::new(config)))
}

/// `prefer` and `require`: accepts any certificate. Handshake signatures are
/// still checked, so the session is bound to the certificate the server sent
/// (which `channel_binding=require` relies on).
#[derive(Debug)]
struct AnyCertificate(WebPkiSupportedAlgorithms);

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.0)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.0)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_schemes()
    }
}

/// `verify-ca`: checks the certificate chain, expiry and usage, but not the
/// host name. Webpki checks the name last, so a name mismatch means
/// everything else passed.
#[derive(Debug)]
struct AnyHostname(Arc<WebPkiServerVerifier>);

impl ServerCertVerifier for AnyHostname {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self.0.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now) {
            Err(rustls::Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) => Ok(ServerCertVerified::assertion()),
            result => result,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::{TlsMode, connector, parse_url};

    fn mode(url: &str) -> TlsMode {
        parse_url(url).unwrap().1
    }

    #[test]
    fn reads_every_sslmode() {
        assert_eq!(mode("postgres://u:p@localhost/db"), TlsMode::Encrypt);
        assert_eq!(mode("postgres://u:p@localhost/db?sslmode=disable"), TlsMode::Disable);
        assert_eq!(mode("postgres://u:p@localhost/db?sslmode=prefer"), TlsMode::Encrypt);
        assert_eq!(mode("postgres://u:p@localhost/db?sslmode=require"), TlsMode::Encrypt);
        assert_eq!(mode("postgres://u:p@localhost/db?sslmode=verify-ca"), TlsMode::VerifyCa);
        assert_eq!(mode("postgres://u:p@localhost/db?sslmode=verify-full"), TlsMode::VerifyFull);
        assert_eq!(
            mode("postgres://u:p@host.neon.tech/db?sslmode=verify-full&channel_binding=require"),
            TlsMode::VerifyFull
        );
        assert_eq!(mode("postgres://u:p@localhost/db?connect_timeout=5&sslmode=verify-ca"), TlsMode::VerifyCa);
        assert_eq!(mode("host=localhost user=u sslmode=verify-full dbname=db"), TlsMode::VerifyFull);
        assert_eq!(mode("host=localhost user=u sslmode=disable"), TlsMode::Disable);
    }

    #[test]
    fn verify_modes_reach_tokio_postgres_as_require() {
        let (config, _) = parse_url("postgres://u:p@localhost/db?sslmode=verify-full").unwrap();
        assert_eq!(config.get_ssl_mode(), tokio_postgres::config::SslMode::Require);
        assert_eq!(config.get_dbname(), Some("db"));
    }

    #[test]
    fn matches_whole_parameters_only() {
        // A password that merely contains the text is not the parameter.
        let (config, tls) = parse_url("postgres://u:xsslmode=verify-full@localhost/db").unwrap();
        assert_eq!(tls, TlsMode::Encrypt);
        assert_eq!(config.get_password(), Some(&b"xsslmode=verify-full"[..]));
        assert!(parse_url("postgres://u:p@localhost/db?sslmode=verify-fullx").is_err());
    }

    #[test]
    fn rejects_unknown_sslmodes() {
        let error = parse_url("postgres://u:p@localhost/db?sslmode=allow").unwrap_err();
        assert!(error.to_string().contains("Invalid postgres url"), "{error}");
    }

    #[test]
    fn builds_a_connector_for_every_mode() {
        assert!(connector(TlsMode::Disable).unwrap().is_none());
        for mode in [TlsMode::Encrypt, TlsMode::VerifyCa, TlsMode::VerifyFull] {
            assert!(connector(mode).unwrap().is_some());
        }
    }
}
