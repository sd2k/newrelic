/*!
*/
#![deny(missing_docs)]

#[macro_use]
extern crate derive_more;

mod app;
mod error;
mod event;
mod segment;
mod transaction;

pub use log::Level as LogLevel;

pub use app::{App, AppConfig, LogOutput, NewRelicConfig};
pub use error::{Error, Result};
pub use event::CustomEvent;
pub use segment::{
    Datastore, DatastoreParams, DatastoreParamsBuilder, ExternalParams, ExternalParamsBuilder,
    Segment,
};
pub use transaction::{Attribute, Transaction};
