//! HTTP client implementation of [`KvStore`].
//!
//! Connects to a remote KV service (e.g. `anatta-kv-server`).

use reqwest::Client;

use crate::error::KvError;
use crate::store::KvStore;

/// [`KvStore`] backed by an HTTP KV service.
#[derive(Clone)]
pub struct HttpKvStore {
    client: Client,
    base_url: String,
}

impl HttpKvStore {
    /// Create a new HTTP KV client pointing at `base_url` (e.g. `http://kv:8080`).
    pub fn new(base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .connect_timeout(std::time::Duration::from_secs(5))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

impl KvStore for HttpKvStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        let resp = self
            .client
            .get(format!("{}/kv/{key}", self.base_url))
            .send()
            .await
            .map_err(|e| KvError::Backend(e.to_string()))?;

        match resp.status().as_u16() {
            200 => {
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| KvError::Backend(e.to_string()))?;
                Ok(Some(bytes.to_vec()))
            }
            404 => Ok(None),
            status => Err(KvError::Backend(format!("kv service returned {status}"))),
        }
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), KvError> {
        let resp = self
            .client
            .put(format!("{}/kv/{key}", self.base_url))
            .body(value)
            .send()
            .await
            .map_err(|e| KvError::Backend(e.to_string()))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(KvError::Backend(format!(
                "kv put returned {}",
                resp.status()
            )))
        }
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        let resp = self
            .client
            .delete(format!("{}/kv/{key}", self.base_url))
            .send()
            .await
            .map_err(|e| KvError::Backend(e.to_string()))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(KvError::Backend(format!(
                "kv delete returned {}",
                resp.status()
            )))
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, KvError> {
        let resp = self
            .client
            .head(format!("{}/kv/{key}", self.base_url))
            .send()
            .await
            .map_err(|e| KvError::Backend(e.to_string()))?;

        match resp.status().as_u16() {
            200 => Ok(true),
            404 => Ok(false),
            status => Err(KvError::Backend(format!("kv exists returned {status}"))),
        }
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, KvError> {
        let resp = self
            .client
            .get(format!("{}/kv", self.base_url))
            .query(&[("prefix", prefix)])
            .send()
            .await
            .map_err(|e| KvError::Backend(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(KvError::Backend(format!(
                "kv scan returned {}",
                resp.status()
            )));
        }

        let items: Vec<ScanEntry> = resp
            .json()
            .await
            .map_err(|e| KvError::Serialization(e.to_string()))?;

        items
            .into_iter()
            .map(|entry| {
                let bytes = base64_decode(&entry.value)?;
                Ok((entry.key, bytes))
            })
            .collect()
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "Keep store operations deferred until the returned future is polled"
    )]
    async fn scan_keys(
        &self,
        _prefix: &str,
        _after: Option<&str>,
        _limit: usize,
    ) -> Result<Vec<String>, KvError> {
        Err(KvError::Unsupported("scan_keys over HTTP".into()))
    }
}

#[derive(serde::Deserialize)]
struct ScanEntry {
    key: String,
    value: String,
}

#[allow(clippy::cast_possible_truncation)]
fn base64_decode(input: &str) -> Result<Vec<u8>, KvError> {
    const DECODE: [u8; 128] = {
        let mut table = [255u8; 128];
        let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0;
        while i < 64 {
            table[chars[i] as usize] = i as u8;
            i += 1;
        }
        table
    };

    let input = input.as_bytes();
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &b in input {
        if b == b'=' {
            break;
        }
        let val = if (b as usize) < 128 {
            DECODE[b as usize]
        } else {
            255
        };
        if val == 255 {
            return Err(KvError::Serialization(format!("invalid base64 byte: {b}")));
        }
        buf = (buf << 6) | u32::from(val);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(out)
}
