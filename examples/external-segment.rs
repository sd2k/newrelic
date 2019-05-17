use std::{env, thread, time::Duration};

use env_logger::init;
use newrelic::{App, ExternalParamsBuilder, NewRelicConfig};

fn main() {
    init();
    NewRelicConfig::default().init().unwrap();

    let license_key =
        env::var("NEW_RELIC_LICENSE_KEY").unwrap_or_else(|_| "example-license-key".to_string());
    let app = App::new("my app", &license_key).expect("Could not create app");

    // Start a web transaction and a segment
    let transaction = app
        .web_transaction("Transaction name")
        .expect("Could not start transaction");
    let segment_params = ExternalParamsBuilder::new("https://www.rust-lang.org/")
        .procedure("GET")
        .library("reqwest")
        .build()
        .expect("Invalid external segment parameters");
    let value = transaction.external_segment(segment_params, |_| {
        // Interesting application code happens here
        thread::sleep(Duration::from_secs(1));
        5
    });
    println!("{}", value);

    // Transaction ends automatically.

    // App is destroyed automatically.
}
