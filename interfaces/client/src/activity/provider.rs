use crate::activity::web::{QueryParamsBuilder, WebClient};
use crate::error::Error;
use crate::Result;
use futures::compat::Future01CompatExt;
use futures::prelude::*;
use std::mem;
use ya_model::activity::{ActivityState, ActivityUsage, ProviderEvent};

pub mod gsb {
    use crate::Result;
    use ya_model::activity::{
        ActivityState, ActivityUsage, ExeScriptBatch, ExeScriptCommandResult, ExeScriptCommandState,
    };

    pub struct GsbProviderApi;

    impl GsbProviderApi {
        pub async fn exec(
            &self,
            _activity_id: &str,
            _batch_id: &str,
            _exe_script: ExeScriptBatch,
        ) -> Result<Vec<ExeScriptCommandResult>> {
            unimplemented!()
        }

        pub async fn get_running_command(
            &self,
            _activity_id: &str,
        ) -> Result<ExeScriptCommandState> {
            unimplemented!()
        }

        pub async fn get_state(&self, _activity_id: &str) -> Result<ActivityState> {
            unimplemented!()
        }

        pub async fn get_usage(&self, _activity_id: &str) -> Result<ActivityUsage> {
            unimplemented!()
        }
    }
}

pub struct ProviderApiClient {
    client: WebClient,
}

impl ProviderApiClient {
    pub fn new(client: WebClient) -> Self {
        Self { client }
    }

    pub fn replace_client(&mut self, client: WebClient) {
        mem::replace(&mut self.client, client);
    }
}

impl ProviderApiClient {
    pub async fn get_activity_events(&self, timeout: Option<i32>) -> Result<Vec<ProviderEvent>> {
        let params = QueryParamsBuilder::new().put("timeout", timeout).build();
        let url = format!("{}/events?{}", self.client.endpoint, params);

        let mut response = self
            .client
            .awc
            .get(&url)
            .send()
            .compat()
            .map_err(Error::from)
            .await?;

        match response.json().compat().await {
            Ok(result) => Ok(result),
            Err(e) => Err(Error::from(e).into()),
        }
    }

    pub async fn set_activity_state(&self, activity_id: &str, state: ActivityState) -> Result<()> {
        let url = format!("{}/activity/{}/state", self.client.endpoint, activity_id);

        self.client
            .awc
            .put(&url)
            .send_json(&state)
            .compat()
            .map_err(Error::from)
            .await?;

        Ok(())
    }

    pub async fn set_activity_usage(&self, activity_id: &str, usage: ActivityUsage) -> Result<()> {
        let url = format!("{}/activity/{}/usage", self.client.endpoint, activity_id);

        self.client
            .awc
            .put(&url)
            .send_json(&usage)
            .compat()
            .map_err(Error::from)
            .await?;

        Ok(())
    }
}
