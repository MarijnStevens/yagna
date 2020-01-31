use serde_json;
use std::{
    thread,
    time::{Duration, SystemTime},
};

use ya_client::{
    market::{ApiClient, RequestorApi},
    web::WebClient,
    Error, Result,
};
use ya_model::market::{Agreement, Demand, RequestorEvent};

async fn query_events(client: &RequestorApi, subscription_id: &str) -> Result<Vec<RequestorEvent>> {
    let mut requestor_events = vec![];

    while requestor_events.is_empty() {
        requestor_events = client.collect(&subscription_id, Some(1), Some(2)).await?;

        println!("Waiting for events");
        thread::sleep(Duration::from_millis(3000));
    }

    println!("{} events found.", requestor_events.len());
    return Ok(requestor_events);
}

async fn wait_for_approval(client: &RequestorApi, proposal_id: &str) {
    loop {
        println!("Waiting for Agreement approval...");

        let _ = match client.wait_for_approval(proposal_id).await {
            Err(Error::TimeoutError { .. }) => {
                println!("Timeout waiting for Agreement approval...");
                Ok("".into())
            }
            Ok(_) => {
                println!("OK! Agreement {} approved by Provider.", proposal_id);
                return;
            }
            e => e,
        };
    }
}

async fn simulate_requestor(client: &RequestorApi) -> Result<()> {
    let demand = Demand::new(serde_json::json!({}), "(&(cpu.architecture=wasm32))".into());
    let subscription_id = client.subscribe(&demand).await?;

    println!("Demand created. Subscription_id {}.", &subscription_id);

    let requestor_events = query_events(client, &subscription_id).await?;

    let RequestorEvent::OfferEvent { offer, .. } = &requestor_events[0];
    let offer = offer.as_ref().unwrap();

    println!("Received offer {}. Sending agreeement.", &offer.id);

    let now = format!("{}", humantime::format_rfc3339_seconds(SystemTime::now()));
    let agreement = Agreement::new(offer.id.clone(), now);
    let _res = client.create_agreement(&agreement).await?;

    println!("Confirm agreement {}.", &agreement.proposal_id);
    let _res = client.confirm_agreement(&agreement.proposal_id).await?;

    println!(
        "Waiting for approval of agreement {}.",
        &agreement.proposal_id
    );

    wait_for_approval(client, &agreement.proposal_id).await;
    client.unsubscribe(&subscription_id).await?;

    Ok(())
}

async fn async_main(api: ApiClient) {
    if let Err(error) = simulate_requestor(api.requestor()).await {
        println!("Error: {}", error);
    };
}

fn main() {
    let client = ApiClient::new(WebClient::builder()).unwrap();
    actix_rt::System::new("test").block_on(async_main(client));
}