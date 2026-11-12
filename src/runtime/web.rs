//! Restricted read-only web capability. Data returned by a site is never policy.
use crate::domain::cognition::CapabilityInfo;
use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::IpAddr;

pub const WEB_ID: &str = "web.read_allowed";
pub const WEB_VERSION: &str = "1";
pub const MAX_WEB_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebError {
    InvalidInput,
    HostDenied,
    AddressDenied,
    Dns,
    Transport,
    Timeout,
    HttpStatus,
    Redirect,
    MediaType,
    Encoding,
    TooLarge,
    InvalidText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebFailure {
    pub code: WebError,
    pub http_status: Option<u16>,
}
impl From<WebError> for WebFailure {
    fn from(code: WebError) -> Self {
        Self {
            code,
            http_status: None,
        }
    }
}
impl std::fmt::Display for WebFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "web read failed: {:?}", self.code)
    }
}
impl std::error::Error for WebFailure {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityMode {
    Live,
    Fixture,
}

// Constructed only after URL authorization; no access to arbitrary HTTP headers.
#[derive(Clone)]
pub struct WebTarget {
    pub(crate) url: Url,
    pub(crate) host: String,
}
impl WebTarget {
    pub fn url(&self) -> &str {
        self.url.as_str()
    }
    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn from_input(input: &Value, allowed: &[String]) -> Result<Self, WebFailure> {
        let target = Self::parse(input)?;
        if !allowed.contains(&target.host) {
            return Err(WebError::HostDenied.into());
        }
        Ok(target)
    }
    pub(crate) fn parse(input: &Value) -> Result<Self, WebFailure> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            url: String,
        }
        let input: Input =
            serde_json::from_value(input.clone()).map_err(|_| WebError::InvalidInput)?;
        let raw = &input.url;
        if raw.len() > 4096
            || raw.chars().any(|c| c.is_control() || c.is_whitespace())
            || raw.contains('\\')
        {
            return Err(WebError::InvalidInput.into());
        }
        let authority = raw
            .split_once("://")
            .ok_or(WebError::InvalidInput)?
            .1
            .split(['/', '?', '#'])
            .next()
            .ok_or(WebError::InvalidInput)?;
        if !authority.is_ascii() || authority.contains(['@', '%']) {
            return Err(WebError::InvalidInput.into());
        }
        let url = Url::parse(raw).map_err(|_| WebError::InvalidInput)?;
        let host = url.host_str().ok_or(WebError::InvalidInput)?.to_owned();
        if url.scheme() != "https"
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || host.ends_with('.')
            || host.parse::<IpAddr>().is_ok()
            || host.contains([':', '[', ']'])
        {
            return Err(WebError::InvalidInput.into());
        }
        Ok(Self { url, host })
    }
}

/// Fail closed for special-use and translation/tunnel ranges. Policy derives from
/// IANA special-purpose registries; conservative exclusions also cover exceptions.
pub fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192
                    && ((b == 0 && (c == 0 || c == 2)) || (b == 88 && c == 99) || b == 168))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && (s[1] & 0xf000) == 0)
        }
    }
}

pub fn web_info() -> CapabilityInfo {
    CapabilityInfo{id:WEB_ID.into(),version:WEB_VERSION.into(),available:true,authorized:false,description:"读取配置允许的精确主机上的HTTPS公开文本。参数 {url: HTTPS URL}；仅443、无凭证/代理/重定向，拒绝内网及特殊地址；正文最多1 MiB，完整保存为外部资料，返回首4096字符及document_id供分段读取。不执行网页脚本。".into()}
}

#[derive(Clone, Serialize)]
pub struct WebDocument {
    pub media_type: String,
    pub text: String,
    pub http_status: u16,
}
impl WebDocument {
    pub fn validate(&self) -> Result<(), WebFailure> {
        if !(200..300).contains(&self.http_status) {
            return Err(WebError::HttpStatus.into());
        }
        if self.text.len() > MAX_WEB_BYTES {
            return Err(WebError::TooLarge.into());
        }
        if self.text.is_empty() || self.text.contains('\0') {
            return Err(WebError::InvalidText.into());
        }
        if !matches!(
            self.media_type.as_str(),
            "text/plain"
                | "text/html"
                | "text/markdown"
                | "text/csv"
                | "text/xml"
                | "application/json"
                | "application/xml"
                | "application/xhtml+xml"
        ) {
            return Err(WebError::MediaType.into());
        }
        Ok(())
    }
}

#[async_trait]
pub trait WebReader: Send + Sync {
    fn mode(&self) -> CapabilityMode;
    async fn read(&self, target: &WebTarget) -> Result<WebDocument, WebFailure>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_https_host_policy_rejects_url_bypasses_before_dns() {
        let hosts = vec!["allowed.example".into()];
        for url in [
            "http://allowed.example/",
            "https://allowed.example:444/",
            "https://allowed.example.evil/",
            "https://sub.allowed.example/",
            "https://allowed.example./",
            "https://user:secret@allowed.example/",
            "https://@allowed.example/",
            "https://allowed.example@evil/",
            "https://%61llowed.example/",
            "https://allowed.example/#hidden",
            "https://allowed.example\\@evil/",
            " https://allowed.example/",
            "https://allowed.example/\n",
            "https://127.1/",
            "https://2130706433/",
            "https://0x7f000001/",
            "https://[::1]/",
            "https://[::ffff:127.0.0.1]/",
        ] {
            assert!(
                WebTarget::from_input(&json!({"url":url}), &hosts).is_err(),
                "{url}"
            );
        }
        for ip in ["127.0.0.1", "[2606:4700:4700::1111]"] {
            assert!(
                WebTarget::from_input(&json!({"url":format!("https://{ip}/")}), &[ip.into()])
                    .is_err()
            );
        }
        assert!(
            WebTarget::from_input(
                &json!({"url":"https://ALLOWED.example:443/read?q=known"}),
                &hosts
            )
            .is_ok()
        );
        assert!(
            WebTarget::from_input(
                &json!({"url":"https://allowed.example/","headers":{"Authorization":"secret"}}),
                &hosts
            )
            .is_err()
        );
        assert!(WebTarget::from_input(&json!({"url":"https://allowed.example/"}), &[]).is_err());
    }
    #[test]
    fn special_use_addresses_and_translation_tunnels_are_closed() {
        for ip in [
            "0.0.0.1",
            "10.1.1.1",
            "127.0.0.1",
            "100.64.0.1",
            "100.127.255.255",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.9",
            "192.0.2.1",
            "192.88.99.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:8.8.8.8",
            "64:ff9b::7f00:1",
            "fc00::1",
            "fe80::1",
            "fec0::1",
            "ff00::1",
            "100::1",
            "2001::1",
            "2001:2::1",
            "2001:20::1",
            "2001:db8::1",
            "2002:7f00:1::",
            "3fff:fff::1",
            "5f00::1",
        ] {
            assert!(!public_address(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "100.128.0.1",
            "172.32.0.1",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
        ] {
            assert!(public_address(ip.parse().unwrap()), "{ip}");
        }
    }
}
