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

/// Fetch a single order by ID.
pub async fn get_order(
    client: &AlpacaClient,
    order_id: &str,
) -> Result<AlpacaOrder, FerrumError> {
    client.get(&format!("/v2/orders/{order_id}")).await
}

/// Fetch all open orders.
pub async fn get_open_orders(client: &AlpacaClient) -> Result<Vec<AlpacaOrder>, FerrumError> {
    client.get_with_query("/v2/orders", &[("status", "open"), ("limit", "100")]).await
}
