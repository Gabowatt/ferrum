use reqwest::{Client, header};
use serde::de::DeserializeOwned;
use crate::{config::AppConfig, error::FerrumError};

const DATA_URL: &str = "https://data.alpaca.markets";

/// Thin wrapper around reqwest that handles Alpaca auth headers and base URL switching.
#[derive(Debug, Clone)]
pub struct AlpacaClient {
    http:     Client,
    base_url: String,
    key:      String,
    secret:   String,
}

impl AlpacaClient {
    pub fn new(cfg: &AppConfig) -> Result<Self, FerrumError> {
        let mut headers = header::HeaderMap::new();
        headers.insert("APCA-API-KEY-ID",
            header::HeaderValue::from_str(cfg.active_key())
                .map_err(|e| FerrumError::Config(e.to_string()))?);
        headers.insert("APCA-API-SECRET-KEY",
            header::HeaderValue::from_str(cfg.active_secret())
                .map_err(|e| FerrumError::Config(e.to_string()))?);

        let http = Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            http,
            base_url: cfg.active_base_url().to_string(),
            key:      cfg.active_key().to_string(),
            secret:   cfg.active_secret().to_string(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// GET `{base_url}{path}` and deserialize the JSON response.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, FerrumError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http
            .get(&url)
            .header("APCA-API-KEY-ID", &self.key)
            .header("APCA-API-SECRET-KEY", &self.secret)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FerrumError::Alpaca(format!("{status}: {body}")));
        }

        let value = resp.json::<T>().await?;
        Ok(value)
    }

    /// GET with query parameters.
    pub async fn get_with_query<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T, FerrumError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http
            .get(&url)
            .header("APCA-API-KEY-ID", &self.key)
            .header("APCA-API-SECRET-KEY", &self.secret)
            .query(params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FerrumError::Alpaca(format!("{status}: {body}")));
        }

        Ok(resp.json::<T>().await?)
    }

    /// GET with query parameters against the market data API (data.alpaca.markets).
    /// Use this for all /v2/stocks/* and /v2/snapshots/* endpoints.
    pub async fn get_data_with_query<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T, FerrumError> {
        let url = format!("{DATA_URL}{path}");
        let resp = self.http
            .get(&url)
            .header("APCA-API-KEY-ID", &self.key)
            .header("APCA-API-SECRET-KEY", &self.secret)
            .query(params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FerrumError::Alpaca(format!("{status}: {body}")));
        }

        Ok(resp.json::<T>().await?)
    }

    /// DELETE `{base_url}{path}` — returns Null on 204, otherwise deserializes JSON.
    pub async fn delete(&self, path: &str) -> Result<serde_json::Value, FerrumError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http
            .delete(&url)
            .header("APCA-API-KEY-ID", &self.key)
            .header("APCA-API-SECRET-KEY", &self.secret)
            .send()
            .await?;
        if resp.status() == 204 {
            return Ok(serde_json::Value::Null);
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FerrumError::Alpaca(format!("{status}: {body}")));
        }
        Ok(resp.json::<serde_json::Value>().await?)
    }

    /// POST with a JSON body.
    pub async fn post<B, T>(&self, path: &str, body: &B) -> Result<T, FerrumError>
    where
        B: serde::Serialize,
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http
            .post(&url)
            .header("APCA-API-KEY-ID", &self.key)
            .header("APCA-API-SECRET-KEY", &self.secret)
            .json(body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FerrumError::Alpaca(format!("{status}: {body}")));
        }

        Ok(resp.json::<T>().await?)
    }
}
