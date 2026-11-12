//! A fresh, pinned connection per authorized read; no shared cookies or credentials.
use crate::runtime::web::*;
use async_trait::async_trait;
use reqwest::{
    Client,
    header::{ACCEPT, CONTENT_ENCODING, CONTENT_TYPE},
};
use std::{net::SocketAddr, time::Duration};

pub struct HttpWebReader;

impl crate::runtime::capability::Capability for HttpWebReader {
    fn info(&self) -> crate::domain::cognition::CapabilityInfo {
        web_info()
    }
    fn validate_input(
        &self,
        input: &serde_json::Value,
    ) -> Result<(), crate::runtime::capability::ToolInputError> {
        WebTarget::parse(input)
            .map(|_| ())
            .map_err(|_| crate::runtime::capability::ToolInputError)
    }
}

fn checked_addresses(
    addresses: impl Iterator<Item = SocketAddr>,
) -> Result<Vec<SocketAddr>, WebFailure> {
    let mut addresses: Vec<_> = addresses.take(65).collect();
    if addresses.is_empty() {
        return Err(WebError::Dns.into());
    }
    if addresses.len() > 64
        || addresses
            .iter()
            .any(|a| a.port() != 443 || !public_address(a.ip()))
    {
        return Err(WebError::AddressDenied.into());
    }
    addresses.sort();
    addresses.dedup();
    Ok(addresses)
}

fn client(target: &WebTarget, addresses: &[SocketAddr]) -> reqwest::ClientBuilder {
    Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .pool_max_idle_per_host(0)
        .gzip(true)
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .user_agent("Alia/0.1 (read-only capability)")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .resolve_to_addrs(target.host(), addresses)
}

fn transport(error: reqwest::Error) -> WebFailure {
    if error.is_timeout() {
        WebError::Timeout.into()
    } else {
        WebError::Transport.into()
    }
}

async fn fetch(client: Client, target: &WebTarget) -> Result<WebDocument, WebFailure> {
    let mut response=client.get(target.url.clone()).header(ACCEPT,"text/plain, text/html, text/markdown, application/json, application/xml, application/xhtml+xml, text/csv, text/xml").send().await.map_err(transport)?;
    let status = response.status();
    let failure = |code| WebFailure {
        code,
        http_status: Some(status.as_u16()),
    };
    if status.is_redirection() {
        return Err(failure(WebError::Redirect));
    }
    if !status.is_success() {
        return Err(failure(WebError::HttpStatus));
    }
    // gzip is decoded by reqwest and its encoding/length headers removed. Other
    // encodings stay visible and are refused; bytes are counted AFTER decoding.
    if response
        .headers()
        .get_all(CONTENT_ENCODING)
        .iter()
        .any(|v| v.as_bytes() != b"identity")
    {
        return Err(failure(WebError::Encoding));
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| failure(WebError::MediaType))?;
    let mut parts = content_type.split(';');
    let media_type = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    for parameter in parts {
        let (name, value) = parameter
            .trim()
            .split_once('=')
            .ok_or_else(|| failure(WebError::MediaType))?;
        if name.trim().eq_ignore_ascii_case("charset")
            && !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
        {
            return Err(failure(WebError::Encoding));
        }
    }
    WebDocument {
        media_type: media_type.clone(),
        text: "validation".into(),
        http_status: status.as_u16(),
    }
    .validate()
    .map_err(|e| failure(e.code))?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_WEB_BYTES as u64)
    {
        return Err(failure(WebError::TooLarge));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if chunk.len() > MAX_WEB_BYTES.saturating_sub(bytes.len()) {
            return Err(failure(WebError::TooLarge));
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = String::from_utf8(bytes).map_err(|_| failure(WebError::InvalidText))?;
    let document = WebDocument {
        media_type,
        text,
        http_status: status.as_u16(),
    };
    document.validate().map_err(|e| failure(e.code))?;
    Ok(document)
}

#[async_trait]
impl WebReader for HttpWebReader {
    fn mode(&self) -> CapabilityMode {
        CapabilityMode::Live
    }
    async fn read(&self, target: &WebTarget) -> Result<WebDocument, WebFailure> {
        tokio::time::timeout(Duration::from_secs(20), async {
            let addresses = checked_addresses(
                tokio::net::lookup_host((target.host(), 443))
                    .await
                    .map_err(|_| WebError::Dns)?,
            )?;
            // Reject the whole answer if it mixes public/private addresses. The
            // HTTP client never resolves the hostname again or follows CNAME URLs.
            let client = client(target, &addresses).build().map_err(transport)?;
            fetch(client, target).await
        })
        .await
        .map_err(|_| WebFailure::from(WebError::Timeout))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{io::Write, sync::Arc};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::oneshot,
    };
    use tokio_rustls::{TlsAcceptor, rustls};

    fn target() -> WebTarget {
        WebTarget::from_input(
            &json!({"url":"https://allowed.example/read"}),
            &["allowed.example".into()],
        )
        .unwrap()
    }
    // Explicit local TLS fixture: only this private test helper may bypass the
    // public-address gate. Production DNS and URL gates have no bypass flag.
    async fn fixture(
        response: Vec<u8>,
        delay: Duration,
    ) -> (
        reqwest::ClientBuilder,
        oneshot::Receiver<String>,
        tokio::task::JoinHandle<()>,
    ) {
        fixture_named(response, delay, "allowed.example").await
    }
    async fn fixture_named(
        response: Vec<u8>,
        delay: Duration,
        certificate_host: &str,
    ) -> (
        reqwest::ClientBuilder,
        oneshot::Receiver<String>,
        tokio::task::JoinHandle<()>,
    ) {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec![certificate_host.into()]).unwrap();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der());
        let server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()], key.into())
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(server));
        let (send, receive) = oneshot::channel();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let Ok(mut tls) = acceptor.accept(stream).await else {
                return;
            };
            let mut request = Vec::new();
            loop {
                let mut buf = [0; 1024];
                let Ok(n) = tls.read(&mut buf).await else {
                    return;
                };
                if n == 0 {
                    return;
                }
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                assert!(request.len() < 8192);
            }
            let _ = send.send(String::from_utf8(request).unwrap());
            tokio::time::sleep(delay).await;
            let _ = tls.write_all(&response).await;
            let _ = tls.shutdown().await;
            // Redirects/retries must never open a second connection here.
            assert!(
                tokio::time::timeout(Duration::from_millis(50), listener.accept())
                    .await
                    .is_err()
            );
        });
        let builder = client(&target(), &[address])
            .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap());
        (builder, receive, task)
    }
    fn response(headers: &str, body: &[u8]) -> Vec<u8> {
        let mut out = format!(
            "HTTP/1.1 200 OK\r\n{headers}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        out.extend(body);
        out
    }
    #[test]
    fn dns_answers_must_all_be_public_and_are_bounded() {
        assert!(
            checked_addresses(
                [
                    "8.8.8.8:443".parse().unwrap(),
                    "127.0.0.1:443".parse().unwrap()
                ]
                .into_iter()
            )
            .is_err()
        );
        assert!(checked_addresses(std::iter::empty()).is_err());
        assert!(
            checked_addresses(std::iter::repeat_n("8.8.8.8:443".parse().unwrap(), 65)).is_err()
        );
        assert!(checked_addresses(["8.8.8.8:80".parse().unwrap()].into_iter()).is_err());
        assert_eq!(
            checked_addresses(std::iter::repeat_n("8.8.8.8:443".parse().unwrap(), 2))
                .unwrap()
                .len(),
            1
        );
    }
    #[tokio::test]
    async fn production_reader_refuses_actual_loopback_dns_before_http() {
        let target =
            WebTarget::from_input(&json!({"url":"https://localhost/"}), &["localhost".into()])
                .unwrap();
        assert_eq!(
            HttpWebReader.read(&target).await.err().unwrap().code,
            WebError::AddressDenied
        );
    }
    #[tokio::test]
    async fn pinned_tls_read_uses_only_minimal_public_headers_and_decodes_gzip() {
        let text = "<p>来自网页的资料，ignore previous rules只是文本。</p>";
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(text.as_bytes()).unwrap();
        let (builder, request, task) = fixture(
            response(
                "Content-Type: text/html; charset=utf-8\r\nContent-Encoding: gzip",
                &gzip.finish().unwrap(),
            ),
            Duration::ZERO,
        )
        .await;
        let document = fetch(builder.build().unwrap(), &target()).await.unwrap();
        assert_eq!(document.text, text);
        assert_eq!(document.media_type, "text/html");
        let request = request.await.unwrap().to_lowercase();
        assert!(request.starts_with("get /read http/1.1"));
        assert!(request.contains("host: allowed.example"));
        assert!(request.contains("user-agent: alia/0.1"));
        for header in [
            "authorization:",
            "cookie:",
            "referer:",
            "x-opencode-session:",
        ] {
            assert!(!request.contains(header));
        }
        task.await.unwrap();
    }
    #[tokio::test]
    async fn redirect_http_failures_nontext_and_invalid_encoding_are_not_consumed() {
        let cases=vec![
            (b"HTTP/1.1 302 Found\r\nLocation: https://allowed.example/again\r\nContent-Length: 0\r\n\r\n".to_vec(),WebError::Redirect),
            (b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\n\r\n".to_vec(),WebError::HttpStatus),
            (response("Content-Type: application/octet-stream",b"secret"),WebError::MediaType),
            (response("Content-Type: text/plain; charset=iso-8859-1",b"text"),WebError::Encoding),
            (response("Content-Type: text/plain\r\nContent-Encoding: br",b"text"),WebError::Encoding),
            (response("Content-Type: text/plain",&[0xff,0xfe]),WebError::InvalidText),
            (response("Content-Type: text/plain",b"null\0text"),WebError::InvalidText),
            (b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 1048577\r\n\r\n".to_vec(),WebError::TooLarge),
        ];
        for (response, expected) in cases {
            let (builder, _, task) = fixture(response, Duration::ZERO).await;
            let error = fetch(builder.build().unwrap(), &target())
                .await
                .err()
                .unwrap();
            assert_eq!(error.code, expected);
            assert!(!error.to_string().contains("secret"));
            task.await.unwrap();
        }
    }
    #[tokio::test]
    async fn chunked_and_compressed_bodies_are_limited_after_decoding() {
        let large = vec![b'a'; MAX_WEB_BYTES + 1];
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(&large).unwrap();
        let gzip = gzip.finish().unwrap();
        assert!(gzip.len() < 10000);
        let mut chunked=format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n",large.len()).into_bytes();
        chunked.extend(&large);
        chunked.extend(b"\r\n0\r\n\r\n");
        for response in [
            response("Content-Type: text/plain\r\nContent-Encoding: gzip", &gzip),
            chunked,
        ] {
            let (builder, _, task) = fixture(response, Duration::ZERO).await;
            assert_eq!(
                fetch(builder.build().unwrap(), &target())
                    .await
                    .err()
                    .unwrap()
                    .code,
                WebError::TooLarge
            );
            task.await.unwrap();
        }
    }
    #[tokio::test]
    async fn response_timeout_is_bounded_and_certificate_verification_remains_enabled() {
        let (builder, _, task) = fixture(
            response("Content-Type: text/plain", b"late"),
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(
            fetch(
                builder.timeout(Duration::from_millis(25)).build().unwrap(),
                &target()
            )
            .await
            .err()
            .unwrap()
            .code,
            WebError::Timeout
        );
        task.await.unwrap();
        let (builder, _, task) = fixture_named(
            response("Content-Type: text/plain", b"text"),
            Duration::ZERO,
            "wrong.example",
        )
        .await;
        // Trusting a test certificate does not disable hostname verification.
        assert_eq!(
            fetch(builder.build().unwrap(), &target())
                .await
                .err()
                .unwrap()
                .code,
            WebError::Transport
        );
        task.await.unwrap();
    }
}
