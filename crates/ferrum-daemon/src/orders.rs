use serde::{Deserialize, Serialize};
use ferrum_core::{client::AlpacaClient, error::FerrumError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlpacaOrderRequest {
    pub symbol:        String,
    pub qty:           String,
    pub side:          String,
    #[serde(rename = "type")]
    pub order_type:    String,
    pub time_in_force: String,
    pub limit_price:   String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlpacaOrder {
    pub id:               String,
    pub status:           String,
    pub symbol:           String,
    pub qty:              String,
    pub side:             String,
    pub filled_qty:       String,
    pub filled_avg_price: Option<String>,
}

pub async fn submit_limit_order(
    client: &AlpacaClient,
    contract: &str,
    side: &str,
    qty: u32,
    limit_price: f64,
) -> Result<AlpacaOrder, FerrumError> {
    let req = AlpacaOrderRequest {
        symbol:        contract.to_string(),
        qty:           qty.to_string(),
        side:          side.to_string(),
        order_type:    "limit".to_string(),
        time_in_force: "day".to_string(),
        limit_price:   format!("{:.2}", limit_price),
    };
    client.post::<_, AlpacaOrder>("/v2/orders", &req).await
}

pub async fn cancel_order(
    client: &AlpacaClient,
    order_id: &str,
) -> Result<(), FerrumError> {
    // DELETE /v2/orders/{order_id} — returns 204
    let _: serde_json::Value = client
        .delete(&format!("/v2/orders/{order_id}"))
        .await
        .unwrap_or(serde_json::Value::Null);
    Ok(())
}
