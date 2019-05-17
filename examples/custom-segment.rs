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
    let value = transaction.custom_segment("Segment name", "Segment category", |_| {
        // Interesting application code happens here
        thread::sleep(Duration::from_secs(1));
        5
    });
    println!("{}", value);

    // Transaction ends automatically.

    // App is destroyed automatically.
}
