/*!
A Rust wrapper over the New Relic C SDK.

See also the [rocket_newrelic] crate for example integration with the
Rocket web framework.

---

Note: versions 0.1.0 onwards of this crate are completely incompatible
with previous versions as they move away from the deprecated New Relic SDK
to the newer New Relic C SDK. This has additional requirements: see
https://docs.newrelic.com/docs/agents/c-sdk/get-started/introduction-c-sdk
for details.

In particular, the New Relic SDK will not link against musl - see the [newrelic-sys] crate for more details.

See https://github.com/hjr3/newrelic-rs for the <0.1.0 repository.

## Usage

Add this crate to your `Cargo.toml`:

```toml
[dependencies]
new-relic = "0.2"
```

You can then instrument your code as follows:

```rust
use std::{env, thread, time::Duration};

use newrelic::{App, NewRelicConfig, LogLevel, LogOutput};

# NewRelicConfig::default()
# .logging(LogLevel::Debug, LogOutput::StdErr)
# .init();

let license_key =
    env::var("NEW_RELIC_LICENSE_KEY").expect("NEW_RELIC_LICENSE_KEY is required");
let app = App::new("my app", &license_key).expect("Could not create app");

let work = || {
    // Start a web transaction and a segment
    let _transaction = app
        .web_transaction("Transaction name")
        .expect("Could not start transaction");
    thread::sleep(Duration::from_secs(1));
    // Transaction ends automatically when dropped.
};

// App is destroyed automatically upon going out of scope.
```

There are several more detailed examples in the [examples] directory of the
crate repository, demonstrating features such as simple and nested segments
and custom events.

This crate still requires the New Relic daemon to be running as per the
[documentation for the New Relic C SDK][c-sdk]; be sure to read this first.

## Async

The [`Segmented`] extension trait adds the ability to run a future inside of a segment.  The feature `async` is required.

## Distributed Tracing

[Distributed tracing][nr-distributed-tracing] is available wiith the feature `distributed_tracing`.  Notably, this feature requires the [libc] crate.

[c-sdk]: https://docs.newrelic.com/docs/agents/c-sdk/get-started/introduction-c-sdk#architecture
[examples]: https://github.com/sd2k/newrelic/tree/master/examples
[newrelic-sys]: https://crates.io/crates/newrelic-sys
[libc]: https://crates.io/crates/libc
[nr-distributed-tracing]: https://docs.newrelic.com/docs/understand-dependencies/distributed-tracing/get-started/introduction-distributed-tracing
[`Segmented`]: ./trait.Segmented.html
[rocket_newrelic]: https://crates.io/crates/rocket_newrelic
*/
#![deny(missing_docs)]

mod app;
mod error;
mod event;
mod segment;
mod transaction;

pub use log::Level as LogLevel;

pub use app::{App, AppBuilder, AppConfig, LogOutput, NewRelicConfig, RecordSQL, TracingThreshold};
pub use error::{Error, Result};
pub use event::CustomEvent;
pub use segment::{
    Datastore, DatastoreParams, DatastoreParamsBuilder, ExternalParams, ExternalParamsBuilder,
    ReferencingSegment, Segment,
};
pub use transaction::{Attribute, Transaction};

#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
mod futures;

#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
pub use futures::{OptionalTransaction, Segmented, SegmentedFuture};
