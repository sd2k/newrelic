use std::{env, thread, time::Duration};

use newrelic::App;

fn main() {
    let license_key =
        env::var("NEW_RELIC_LICENSE_KEY").unwrap_or_else(|_| "example-license-key".to_string());
    let app = App::new("my app", &license_key).expect("Could not create app");

    // Start a web transaction and a segment
    let transaction = app
        .web_transaction("Transaction name")
        .expect("Could not start transaction");
    let event = transaction
        .custom_event("Event name or type")
        .expect("Could not create custom event");
    event
        .add_attribute("number of foos", 1_000)
        .expect("Could not add attribute");
    event.record();
    thread::sleep(Duration::from_secs(1));

    // Transaction ends automatically.

    // App is destroyed automatically.
}
