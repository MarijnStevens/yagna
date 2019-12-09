/*
 * Yagna Activity API
 *
 * It conforms with capability level 1 of the [Activity API specification](https://docs.google.com/document/d/1BXaN32ediXdBHljEApmznSfbuudTU8TmvOmHKl0gmQM).
 *
 * The version of the OpenAPI document: v1
 *
 * Generated by: https://openapi-generator.tech
 */

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActivityUsage {
    /// Current usage vector
    #[serde(rename = "currentUsage", skip_serializing_if = "Option::is_none")]
    pub current_usage: Option<Vec<f64>>,
}

impl ActivityUsage {
    pub fn new(current_usage: Option<Vec<f64>>) -> ActivityUsage {
        ActivityUsage { current_usage }
    }
}
