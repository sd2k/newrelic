use std::{ffi::CString, os::raw::c_char};

use log::{debug, error};
use newrelic_sys as ffi;

use crate::{
    error::{Error, Result},
    transaction::Transaction,
};

/// A segment pointer.
///
/// Lacks a reference to a parent transaction and therefore
/// all methods require the parent.
/// This allows any container with the ability to provide
/// a transaction reference, such as static lifetime
/// constructs like Arc, to wrap this and act as a segment.
///
/// As this lacks a parent transaction, which is required
/// to end a transaction, the wrapping container is responsible
/// for ending the pointer as part of its `Drop` implementation.
/// Failure to do so will leave the segment dangling without
/// end.
///
#[derive(Default)]
struct SegmentPointer {
    /// This holds an unsafe reference to a raw Segment.
    inner: Option<*mut ffi::newrelic_segment_t>,
}

impl SegmentPointer {
    pub fn custom(
        transaction: impl AsRef<Transaction>,
        name: impl AsRef<str>,
        category: impl AsRef<str>,
    ) -> Result<Self> {
        let transaction = transaction.as_ref();
        let name = name.as_ref();
        let category = category.as_ref();
        let c_name = CString::new(name);
        let c_category = CString::new(category);

        let pointer = match (c_name, c_category) {
            (Ok(c_name), Ok(c_category)) => {
                let pointer = unsafe {
                    ffi::newrelic_start_segment(
                        transaction.inner,
                        c_name.as_ptr(),
                        c_category.as_ptr(),
                    )
                };
                if pointer.is_null() {
                    error!(
                        "Could not create segment with name {} due to invalid transaction",
                        name
                    );
                    Err(Error::SegmentStartError)
                } else {
                    Ok(Self {
                        inner: Some(pointer),
                    })
                }
            }
            _ => {
                error!(
                    "Could not create segment with name {}, category {}, due to NUL string in name or category",
                    name,
                    category,
                );
                Err(Error::SegmentStartError)
            }
        };
        debug!("Created segment");
        pointer
    }

    pub fn datastore(
        transaction: impl AsRef<Transaction>,
        params: impl AsRef<DatastoreParams>,
    ) -> Result<Self> {
        let transaction = transaction.as_ref();
        let params = params.as_ref();
        let pointer =
            unsafe { ffi::newrelic_start_datastore_segment(transaction.inner, &params.as_ptr()) };
        let pointer = if pointer.is_null() {
            error!("Could not create datastore segment due to invalid transaction");
            Err(Error::SegmentStartError)
        } else {
            Ok(Self {
                inner: Some(pointer),
            })
        };
        debug!("Created segment");
        pointer
    }

    pub fn external(
        transaction: impl AsRef<Transaction>,
        params: impl AsRef<ExternalParams>,
    ) -> Result<Self> {
        let transaction = transaction.as_ref();
        let params = params.as_ref();
        debug!("Trying to start external segment");
        let pointer =
            unsafe { ffi::newrelic_start_external_segment(transaction.inner, &params.as_ptr()) };
        let pointer = if pointer.is_null() {
            error!("Could not create external segment due to invalid transaction");
            Err(Error::SegmentStartError)
        } else {
            Ok(Self {
                inner: Some(pointer),
            })
        };
        debug!("Created segment");
        pointer
    }

    pub fn custom_nested(
        &self,
        transaction: impl AsRef<Transaction>,
        name: impl AsRef<str>,
        category: impl AsRef<str>,
    ) -> Result<Self> {
        let inner = self.inner.ok_or_else(|| {
            error!("Could not create external segment due to invalid parent segment");
            Error::SegmentStartError
        })?;
        let transaction = transaction.as_ref();
        let name = name.as_ref();
        let category = category.as_ref();
        let nested_pointer = Self::custom(transaction, name, category)?;
        // If result is ok, then guaranteed there is some pointer
        let nested_inner = nested_pointer.inner.unwrap();
        unsafe {
            ffi::newrelic_set_segment_parent(nested_inner, inner);
        }
        Ok(nested_pointer)
    }

    pub fn datastore_nested(
        &self,
        transaction: impl AsRef<Transaction>,
        params: impl AsRef<DatastoreParams>,
    ) -> Result<Self> {
        let inner = self.inner.ok_or_else(|| {
            error!("Could not create external segment due to invalid parent segment");
            Error::SegmentStartError
        })?;
        let transaction = transaction.as_ref();
        let params = params.as_ref();
        let nested_pointer = Self::datastore(transaction, params)?;
        // If result is ok, then guaranteed there is some pointer
        let nested_inner = nested_pointer.inner.unwrap();
        unsafe {
            ffi::newrelic_set_segment_parent(nested_inner, inner);
        }
        Ok(nested_pointer)
    }

    pub fn external_nested(
        &self,
        transaction: impl AsRef<Transaction>,
        params: impl AsRef<ExternalParams>,
    ) -> Result<Self> {
        let inner = self.inner.ok_or_else(|| {
            error!("Could not create external segment due to invalid parent segment");
            Error::SegmentStartError
        })?;
        let transaction = transaction.as_ref();
        let params = params.as_ref();
        let nested_pointer = Self::external(transaction, params)?;
        // If result is ok, then guaranteed there is some pointer
        let nested_inner = nested_pointer.inner.unwrap();
        unsafe {
            ffi::newrelic_set_segment_parent(nested_inner, inner);
        }
        Ok(nested_pointer)
    }

    #[cfg(feature = "distributed_tracing")]
    pub fn distributed_trace(&self, transaction: impl AsRef<Transaction>) -> Option<String> {
        let transaction = transaction.as_ref();
        self.inner.map(|pointer| {
            let payload = FreeableString::new(unsafe {
                ffi::newrelic_create_distributed_trace_payload_httpsafe(transaction.inner, pointer)
            });
            payload.convert()
        })
    }

    pub fn end(&mut self, transaction: impl AsRef<Transaction>) {
        if let Some(mut inner) = self.inner {
            let transaction = transaction.as_ref();
            unsafe {
                ffi::newrelic_end_segment(transaction.inner, &mut inner);
            }
            debug!("Ended segment");
            self.inner = None;
        }
    }
}

unsafe impl Send for SegmentPointer {}

unsafe impl Sync for SegmentPointer {}

/// A segment with a reference to a transaction via the `AsRef` trait.
///
/// Use segments to instrument transactions with greater granularity.
///
/// `ReferencingSegment` can be nested by calling the various `_nested` methods on
/// an existing borrowing segment.
pub struct ReferencingSegment<T: AsRef<Transaction> + Clone> {
    /// A reference to a parental transaction as mediated by the `Borrow` trait.
    transaction: T,
    /// A
    segment_pointer: SegmentPointer,
}

impl<T: AsRef<Transaction> + Clone> ReferencingSegment<T> {
    /// Create a custom segment within this transaction.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ReferencingSegment};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Test transaction")
    ///     .expect("Could not start transaction");
    /// {
    ///     let _ = ReferencingSegment::custom(&transaction, "Test segment", "Test category")
    ///         .expect("Could not start segment");
    ///     thread::sleep(Duration::from_secs(1))
    /// }
    /// ```
    pub fn custom(
        transaction: T,
        name: impl AsRef<str>,
        category: impl AsRef<str>,
    ) -> Result<Self> {
        let segment_pointer = SegmentPointer::custom(transaction.as_ref(), name, category)?;
        Ok(Self {
            transaction,
            segment_pointer,
        })
    }

    /// Create a datastore segment within a given transaction.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ReferencingSegment, Datastore, DatastoreParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Test transaction")
    ///     .expect("Could not start transaction");
    /// let segment_params = DatastoreParamsBuilder::new(Datastore::Postgres)
    ///     .collection("people")
    ///     .operation("select")
    ///     .build()
    ///     .expect("Invalid datastore segment parameters");
    /// {
    ///     let _ = ReferencingSegment::datastore(&transaction, &segment_params)
    ///         .expect("Could not start segment");
    ///     thread::sleep(Duration::from_secs(1))
    /// }
    /// ```
    pub fn datastore(transaction: T, params: impl AsRef<DatastoreParams>) -> Result<Self> {
        let segment_pointer = SegmentPointer::datastore(transaction.as_ref(), params)?;
        Ok(Self {
            transaction,
            segment_pointer,
        })
    }

    /// Create an external segment within a given transaction.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ExternalParamsBuilder, ReferencingSegment};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Test transaction")
    ///     .expect("Could not start transaction");
    /// let segment_params = ExternalParamsBuilder::new("https://www.rust-lang.org/")
    ///     .procedure("GET")
    ///     .library("reqwest")
    ///     .build()
    ///     .expect("Invalid external segment parameters");
    /// {
    ///     let _ = ReferencingSegment::external(&transaction, &segment_params)
    ///         .expect("Could not start segment");
    ///     thread::sleep(Duration::from_secs(1))
    /// }
    /// ```
    pub fn external(transaction: T, params: impl AsRef<ExternalParams>) -> Result<Self> {
        let segment_pointer = SegmentPointer::external(transaction.as_ref(), params)?;
        Ok(Self {
            transaction,
            segment_pointer,
        })
    }

    /// Create a new segment nested within this one.
    ///
    /// `name` and `category` will have any null bytes removed before
    /// creating the segment.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ReferencingSegment};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = {
    ///     let s = ReferencingSegment::custom(&transaction, "Segment name", "Segment category")
    ///         .expect("Could not start segment");
    ///     thread::sleep(Duration::from_secs(1));
    ///     let expensive_val_1 = s.custom_nested("First nested segment", "Nested category", |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         3
    ///     }).expect("Could not start nested segment");
    ///     let expensive_val_2 = s.custom_nested("Second nested segment", "Nested category", |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         2
    ///     }).expect("Could not start nested segment");
    ///     expensive_val_1 * expensive_val_2
    /// };
    /// ```
    pub fn custom_nested<F, V>(
        &self,
        name: impl AsRef<str>,
        category: impl AsRef<str>,
        func: F,
    ) -> Result<V>
        where
            F: FnOnce(ReferencingSegment<T>) -> V,
    {
        Ok(func(self.create_custom_nested(name, category)?))
    }

    /// Create a new datastore segment nested within this one.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ReferencingSegment, Datastore, DatastoreParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = {
    ///     let s = ReferencingSegment::custom(&transaction, "Segment name", "Segment category")
    ///         .expect("Could not start segment");
    ///     thread::sleep(Duration::from_secs(1));
    ///     let datastore_segment_params = DatastoreParamsBuilder::new(Datastore::Postgres)
    ///         .collection("people")
    ///         .operation("select")
    ///         .build()
    ///         .expect("Invalid datastore segment parameters");
    ///     let expensive_val = s.datastore_nested(&datastore_segment_params, |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         3
    ///     }).expect("Could not start nested segment");
    ///     expensive_val * 2
    /// };
    /// ```
    pub fn datastore_nested<F, V>(&self, params: impl AsRef<DatastoreParams>, func: F) -> Result<V>
        where
            F: FnOnce(ReferencingSegment<T>) -> V,
    {
        Ok(func(self.create_datastore_nested(params)?))
    }

    /// Create a new external segment nested within this one.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ReferencingSegment, ExternalParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = {
    ///     let s = ReferencingSegment::custom(&transaction, "Segment name", "Segment category")
    ///         .expect("Could not start segment");
    ///     thread::sleep(Duration::from_secs(1));
    ///     let external_segment_params = ExternalParamsBuilder::new("https://www.rust-lang.org/")
    ///         .procedure("GET")
    ///         .library("reqwest")
    ///         .build()
    ///         .expect("Invalid external segment parameters");
    ///     let expensive_val = s.external_nested(&external_segment_params, |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         3
    ///     }).expect("Could not start nested segment");
    ///     expensive_val * 2
    /// };
    /// ```
    pub fn external_nested<F, V>(&self, params: impl AsRef<ExternalParams>, func: F) -> Result<V>
        where
            F: FnOnce(ReferencingSegment<T>) -> V,
    {
        Ok(func(self.create_external_nested(params)?))
    }

    /// Create a new segment nested within this one.
    ///
    /// `name` and `category` will have any null bytes removed before
    /// creating the segment.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ReferencingSegment};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = {
    ///     let s = ReferencingSegment::custom(&transaction, "Segment name", "Segment category")
    ///         .expect("Could not start segment");
    ///     thread::sleep(Duration::from_secs(1));
    ///     let _ = s.create_custom_nested("Second nested segment", "Nested category")
    ///         .expect("Could not start nested segment");
    ///     thread::sleep(Duration::from_secs(1));
    /// };
    /// ```
    pub fn create_custom_nested(
        &self,
        name: impl AsRef<str>,
        category: impl AsRef<str>,
    ) -> Result<Self> {
        let sp = self.segment_pointer.custom_nested(
            self.transaction.as_ref(),
            name.as_ref(),
            category.as_ref(),
        )?;
        let transaction = self.transaction.clone();
        Ok(Self {
            segment_pointer: sp,
            transaction,
        })
    }

    /// Create a new datastore segment nested within this one.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ReferencingSegment, Datastore, DatastoreParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = {
    ///     let s = ReferencingSegment::custom(&transaction, "Segment name", "Segment category")
    ///         .expect("Could not start segment");
    ///     thread::sleep(Duration::from_secs(1));
    ///     let datastore_segment_params = DatastoreParamsBuilder::new(Datastore::Postgres)
    ///         .collection("people")
    ///         .operation("select")
    ///         .build()
    ///         .expect("Invalid datastore segment parameters");
    ///     let _ = s.create_datastore_nested(&datastore_segment_params)
    ///         .expect("Could not start nested segment");
    ///     thread::sleep(Duration::from_secs(1));
    /// };
    /// ```
    pub fn create_datastore_nested(&self, params: impl AsRef<DatastoreParams>) -> Result<Self> {
        let sp = self
            .segment_pointer
            .datastore_nested(self.transaction.as_ref(), params.as_ref())?;
        let transaction = self.transaction.clone();
        Ok(Self {
            segment_pointer: sp,
            transaction,
        })
    }

    /// Create a new external segment nested within this one.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ReferencingSegment, ExternalParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = {
    ///     let s = ReferencingSegment::custom(&transaction, "Segment name", "Segment category")
    ///         .expect("Could not start segment");
    ///     thread::sleep(Duration::from_secs(1));
    ///     let external_segment_params = ExternalParamsBuilder::new("https://www.rust-lang.org/")
    ///         .procedure("GET")
    ///         .library("reqwest")
    ///         .build()
    ///         .expect("Invalid external segment parameters");
    ///     let _ = s.create_external_nested(&external_segment_params)
    ///         .expect("Could not start nested segment");
    ///     thread::sleep(Duration::from_secs(1));
    /// };
    /// ```
    pub fn create_external_nested(&self, params: impl AsRef<ExternalParams>) -> Result<Self> {
        let sp = self
            .segment_pointer
            .external_nested(self.transaction.as_ref(), params.as_ref())?;
        let transaction = self.transaction.clone();
        Ok(Self {
            segment_pointer: sp,
            transaction,
        })
    }

    /// Create a distributed trace payload, a base64-encoded string, to add to a service's outbound
    /// requests.
    ///
    /// This payload contains the metadata necessary to link spans together for a complete
    /// distributed trace. The metadata includes: the trace ID number, the span ID number, New
    /// Relic account ID number, and sampling information. Note that a payload must be created
    /// within an active transaction.
    ///
    /// This is normally included in the "newrelic" header of an outbound http request.
    ///
    /// See the [newrelic site] for more information on distributed tracing.
    ///
    /// Example:
    ///
    /// ```rust
    /// # use newrelic::Error;
    /// # fn main() -> Result<(), Error> {
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{AppBuilder, ReferencingSegment, ExternalParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = AppBuilder::new("my app", &license_key)?
    ///     .distributed_tracing(true)
    ///     .build()?;
    /// let transaction = app
    ///     .web_transaction("Test transaction")
    ///     .expect("Could not start transaction");
    /// let segment_params = ExternalParamsBuilder::new("https://www.rust-lang.org/")
    ///     .procedure("GET")
    ///     .library("reqwest")
    ///     .build()
    ///     .expect("Invalid external segment parameters");
    /// {
    ///     let segment = ReferencingSegment::external(&transaction, &segment_params)
    ///         .expect("Could not start segment");
    ///     let _header = segment.distributed_trace();
    ///     thread::sleep(Duration::from_secs(1))
    /// }
    /// #   Ok(())
    /// # }
    /// ```
    /// [newrelic site]:
    /// https://docs.newrelic.com/docs/understand-dependencies/distributed-tracing/get-started/introduction-distributed-tracing
    #[cfg(feature = "distributed_tracing")]
    #[cfg_attr(docsrs, doc(cfg(feature = "distributed_tracing")))]
    pub fn distributed_trace(&self) -> Option<String> {
        self.segment_pointer
            .distributed_trace(self.transaction.as_ref())
    }

    /// Explicitly end this segment.
    ///
    /// If this is not called, the segment is automatically ended
    /// when dropped.
    pub fn end(&mut self) {
        self.segment_pointer.end(self.transaction.as_ref())
    }
}

impl<T: AsRef<Transaction> + Clone> Drop for ReferencingSegment<T> {
    fn drop(&mut self) {
        self.end();
    }
}

/// A segment within a transaction.
///
/// Use segments to instrument transactions with greater granularity.
/// Segments are created using the various methods on a `Transaction`.
///
/// Segments can be nested by calling the various `_nested` methods on
/// an existing segment.
#[derive(Default)]
pub struct Segment<'a> {
    /// This holds either the actual segment, if creation was successful,
    /// or None, if creation failed.
    /// This means users don't have to deal with Results and segment
    /// creation can fail quietly. Usually this would be bad, but we probably
    /// just want to continue even if New Relic monitoring isn't working...
    /// right?
    inner: Option<ReferencingSegment<&'a Transaction>>,
}

impl<'a> Segment<'a> {
    pub(crate) fn custom(transaction: &'a Transaction, name: &str, category: &str) -> Self {
        Self {
            inner: ReferencingSegment::custom(transaction, name, category).ok(),
        }
    }

    pub(crate) fn datastore(transaction: &'a Transaction, params: &DatastoreParams) -> Self {
        Self {
            inner: ReferencingSegment::datastore(transaction, params).ok(),
        }
    }

    pub(crate) fn external(transaction: &'a Transaction, params: &ExternalParams) -> Self {
        Self {
            inner: ReferencingSegment::external(transaction, params).ok(),
        }
    }

    /// Create a new segment nested within this one.
    ///
    /// `name` and `category` will have any null bytes removed before
    /// creating the segment.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::App;
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = transaction.custom_segment("Segment name", "Segment category", |s| {
    ///     thread::sleep(Duration::from_secs(1));
    ///     let expensive_val_1 = s.custom_nested("First nested segment", "Nested category", |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         3
    ///     });
    ///     let expensive_val_2 = s.custom_nested("Second nested segment", "Nested category", |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         2
    ///     });
    ///     expensive_val_1 * expensive_val_2
    /// });
    /// ```
    pub fn custom_nested<F, V>(&self, name: &str, category: &str, func: F) -> V
        where
            F: FnOnce(Segment) -> V,
    {
        func(self.create_custom_nested(name, category))
    }

    /// Create a new datastore segment nested within this one.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, Datastore, DatastoreParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = transaction.custom_segment("Segment name", "Segment category", |s| {
    ///     thread::sleep(Duration::from_secs(1));
    ///     let datastore_segment_params = DatastoreParamsBuilder::new(Datastore::Postgres)
    ///         .collection("people")
    ///         .operation("select")
    ///         .build()
    ///         .expect("Invalid datastore segment parameters");
    ///     let expensive_val = s.datastore_nested(&datastore_segment_params, |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         3
    ///     });
    ///     expensive_val * 2
    /// });
    /// ```
    pub fn datastore_nested<F, V>(&self, params: &DatastoreParams, func: F) -> V
        where
            F: FnOnce(Segment) -> V,
    {
        func(self.create_datastore_nested(params))
    }

    /// Create a new external segment nested within this one.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ExternalParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = transaction.custom_segment("Segment name", "Segment category", |s| {
    ///     thread::sleep(Duration::from_secs(1));
    ///     let external_segment_params = ExternalParamsBuilder::new("https://www.rust-lang.org/")
    ///         .procedure("GET")
    ///         .library("reqwest")
    ///         .build()
    ///         .expect("Invalid external segment parameters");
    ///     let expensive_val = s.external_nested(&external_segment_params, |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         3
    ///     });
    ///     expensive_val * 2
    /// });
    /// ```
    pub fn external_nested<F, V>(&self, params: &ExternalParams, func: F) -> V
        where
            F: FnOnce(Segment) -> V,
    {
        func(self.create_external_nested(params))
    }

    /// Create a new segment nested within this one.
    ///
    /// `name` and `category` will have any null bytes removed before
    /// creating the segment.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::App;
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = transaction.custom_segment("Segment name", "Segment category", |s| {
    ///     thread::sleep(Duration::from_secs(1));
    ///     let expensive_val_1 = s.custom_nested("First nested segment", "Nested category", |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         3
    ///     });
    ///     let expensive_val_2 = s.custom_nested("Second nested segment", "Nested category", |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         2
    ///     });
    ///     expensive_val_1 * expensive_val_2
    /// });
    /// ```
    pub fn create_custom_nested(&self, name: &str, category: &str) -> Self {
        // We can only create a nested segment if this segment is 'real'
        let nested = self
            .inner
            .as_ref()
            .and_then(|inner| inner.create_custom_nested(name, category).ok());
        Self { inner: nested }
    }

    /// Create a new datastore segment nested within this one.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, Datastore, DatastoreParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = transaction.custom_segment("Segment name", "Segment category", |s| {
    ///     thread::sleep(Duration::from_secs(1));
    ///     let datastore_segment_params = DatastoreParamsBuilder::new(Datastore::Postgres)
    ///         .collection("people")
    ///         .operation("select")
    ///         .build()
    ///         .expect("Invalid datastore segment parameters");
    ///     let _ = s.create_datastore_nested(&datastore_segment_params);
    ///     thread::sleep(Duration::from_secs(1));
    /// });
    /// ```
    pub fn create_datastore_nested(&self, params: &DatastoreParams) -> Self {
        // We can only create a nested segment if this segment is 'real'
        let nested = self
            .inner
            .as_ref()
            .and_then(|inner| inner.create_datastore_nested(params).ok());
        Self { inner: nested }
    }

    /// Create a new external segment nested within this one.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ExternalParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key)
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    /// let value = transaction.custom_segment("Segment name", "Segment category", |s| {
    ///     thread::sleep(Duration::from_secs(1));
    ///     let external_segment_params = ExternalParamsBuilder::new("https://www.rust-lang.org/")
    ///         .procedure("GET")
    ///         .library("reqwest")
    ///         .build()
    ///         .expect("Invalid external segment parameters");
    ///     let _ = s.create_external_nested(&external_segment_params);
    ///     thread::sleep(Duration::from_secs(1));
    /// });
    /// ```
    pub fn create_external_nested(&self, params: &ExternalParams) -> Self {
        // We can only create a nested segment if this segment is 'real'
        let nested = self
            .inner
            .as_ref()
            .and_then(|inner| inner.create_external_nested(params).ok());
        Self { inner: nested }
    }

    /// Create a distributed trace payload, a base64-encoded string, to add to a service's outbound
    /// requests.
    ///
    /// This payload contains the metadata necessary to link spans together for a complete
    /// distributed trace. The metadata includes: the trace ID number, the span ID number, New
    /// Relic account ID number, and sampling information. Note that a payload must be created
    /// within an active transaction.
    ///
    /// This is normally included in the "newrelic" header of an outbound http request.
    ///
    /// See the [newrelic site] for more information on distributed tracing.
    ///
    /// Example:
    ///
    /// ```rust
    /// # use newrelic::Error;
    /// # fn main() -> Result<(), Error> {
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{AppBuilder, ExternalParamsBuilder};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = AppBuilder::new("my app", &license_key)?
    ///     .distributed_tracing(true)
    ///     .build()?;
    /// let transaction = app
    ///     .web_transaction("Test transaction")
    ///     .expect("Could not start transaction");
    /// let segment_params = ExternalParamsBuilder::new("https://www.rust-lang.org/")
    ///     .procedure("GET")
    ///     .library("reqwest")
    ///     .build()
    ///     .expect("Invalid external segment parameters");
    /// {
    ///     let segment = transaction.create_external_segment(&segment_params);
    ///     let _header = segment.distributed_trace();
    ///     thread::sleep(Duration::from_secs(1))
    /// }
    /// #   Ok(())
    /// # }
    /// ```
    /// [newrelic site]:
    /// https://docs.newrelic.com/docs/understand-dependencies/distributed-tracing/get-started/introduction-distributed-tracing
    #[cfg(feature = "distributed_tracing")]
    #[cfg_attr(docsrs, doc(cfg(feature = "distributed_tracing")))]
    pub fn distributed_trace(&self) -> String {
        self.inner
            .as_ref()
            .and_then(|inner| inner.distributed_trace())
            .unwrap_or("".to_string())
    }

    /// Explicitly end this segment.
    ///
    /// If this is not called, the segment is automatically ended
    /// when dropped.
    pub fn end(&mut self) {
        if let Some(ref mut inner) = self.inner {
            inner.end()
        }
        self.inner = None;
    }
}

impl<'a> Drop for Segment<'a> {
    fn drop(&mut self) {
        self.end();
    }
}

#[cfg(feature = "distributed_tracing")]
#[cfg_attr(docsrs, doc(cfg(feature = "distributed_tracing")))]
struct FreeableString(*mut std::os::raw::c_char);

#[cfg(feature = "distributed_tracing")]
#[cfg_attr(docsrs, doc(cfg(feature = "distributed_tracing")))]
impl FreeableString {
    fn new(inner: *mut std::os::raw::c_char) -> Self {
        Self(inner)
    }

    fn convert(&self) -> String {
        let c_str = unsafe { std::ffi::CStr::from_ptr(self.0) };

        c_str.to_str().unwrap().to_string()
    }
}

#[cfg(feature = "distributed_tracing")]
#[cfg_attr(docsrs, doc(cfg(feature = "distributed_tracing")))]
impl Drop for FreeableString {
    fn drop(&mut self) {
        unsafe {
            libc::free(self.0 as *mut std::ffi::c_void);
        }
    }
}

macro_rules! cstring_or_null_ptr {
    ($param:expr) => {
        match $param {
            Some(p) => CString::new(p)?.into_raw(),
            None => std::ptr::null_mut(),
        }
    };
}

macro_rules! drop_if_non_null {
    ($field:expr) => {
        if !$field.is_null() {
            let _ = CString::from_raw($field);
        }
    };
}

/// Builder for parameters used to instrument external calls.
pub struct ExternalParamsBuilder<'a> {
    uri: &'a str,
    procedure: Option<&'a str>,
    library: Option<&'a str>,
}

impl<'a> ExternalParamsBuilder<'a> {
    /// Begin creating a new set of external parameters.
    pub fn new(uri: &'a str) -> Self {
        ExternalParamsBuilder {
            uri,
            procedure: None,
            library: None,
        }
    }

    /// Set the procedure of the external segment.
    ///
    /// In HTTP contexts, this will usually be the request method
    /// (eg GET, POST, etc). For non-HTTP requests, or protocols that
    /// encode more specific semantics on top of HTTP like SOAP, you
    /// may wish to use a different value that more precisely encodes
    /// how the resource was requested.
    pub fn procedure(mut self, procedure: &'a str) -> Self {
        self.procedure = Some(procedure);
        self
    }

    /// Set the library of the external segment.
    pub fn library(mut self, library: &'a str) -> Self {
        self.library = Some(library);
        self
    }

    /// Consume the builder, returning the set of external parameters.
    ///
    /// This will fail if any of the the parameters contain null bytes.
    pub fn build(self) -> Result<ExternalParams> {
        debug!("Creating ExternalParams");
        let uri = CString::new(self.uri)?.into_raw();
        Ok(ExternalParams {
            uri,
            procedure: cstring_or_null_ptr!(self.procedure),
            library: cstring_or_null_ptr!(self.library),
        })
    }
}

/// Parameters used to instrument external segments.
///
/// Create this using `ExternalParamsBuilder`.
pub struct ExternalParams {
    uri: *mut c_char,
    procedure: *mut c_char,
    library: *mut c_char,
}

impl ExternalParams {
    fn as_ptr(&self) -> ffi::newrelic_external_segment_params_t {
        ffi::newrelic_external_segment_params_t {
            uri: self.uri,
            procedure: self.procedure,
            library: self.library,
        }
    }
}

impl Drop for ExternalParams {
    fn drop(&mut self) {
        debug!("Reclaiming ExternalParams");
        unsafe {
            let _ = CString::from_raw(self.uri);
            drop_if_non_null!(self.procedure);
            drop_if_non_null!(self.library);
        }
    }
}

impl AsRef<Self> for ExternalParams {
    fn as_ref(&self) -> &Self {
        self
    }
}

unsafe impl Send for ExternalParams {}

unsafe impl Sync for ExternalParams {}

/// The datastore type, used when instrumenting a datastore segment.
pub enum Datastore {
    /// Firebird. Uses query instrumentation.
    Firebird,
    /// Informix. Uses query instrumentation.
    Informix,
    /// MSSQL. Uses query instrumentation.
    MSSQL,
    /// MySQL. Uses query instrumentation.
    MySQL,
    /// Oracle. Uses query instrumentation.
    Oracle,
    /// PostgreSQL. Uses query instrumentation.
    Postgres,
    /// SQLite. Uses query instrumentation.
    SQLite,
    /// Sybase. Uses query instrumentation.
    Sybase,
    /// Memcached. Does not use query instrumentation.
    Memcached,
    /// MongoDB. Does not use query instrumentation.
    MongoDB,
    /// ODBC. Does not use query instrumentation.
    ODBC,
    /// Redis. Does not use query instrumentation.
    Redis,
    /// Other. Does not use query instrumentation.
    Other,
}

impl Datastore {
    fn inner(&self) -> *mut c_char {
        let datastore = match self {
            Datastore::Firebird => ffi::NEWRELIC_DATASTORE_FIREBIRD.as_ptr(),
            Datastore::Informix => ffi::NEWRELIC_DATASTORE_INFORMIX.as_ptr(),
            Datastore::MSSQL => ffi::NEWRELIC_DATASTORE_MSSQL.as_ptr(),
            Datastore::MySQL => ffi::NEWRELIC_DATASTORE_MYSQL.as_ptr(),
            Datastore::Oracle => ffi::NEWRELIC_DATASTORE_ORACLE.as_ptr(),
            Datastore::Postgres => ffi::NEWRELIC_DATASTORE_POSTGRES.as_ptr(),
            Datastore::SQLite => ffi::NEWRELIC_DATASTORE_SQLITE.as_ptr(),
            Datastore::Sybase => ffi::NEWRELIC_DATASTORE_SYBASE.as_ptr(),
            Datastore::Memcached => ffi::NEWRELIC_DATASTORE_MEMCACHE.as_ptr(),
            Datastore::MongoDB => ffi::NEWRELIC_DATASTORE_MONGODB.as_ptr(),
            Datastore::ODBC => ffi::NEWRELIC_DATASTORE_ODBC.as_ptr(),
            Datastore::Redis => ffi::NEWRELIC_DATASTORE_REDIS.as_ptr(),
            Datastore::Other => ffi::NEWRELIC_DATASTORE_OTHER.as_ptr(),
        };
        datastore as *mut c_char
    }
}

/// Builder for parameters used to instrument datastore segments.
pub struct DatastoreParamsBuilder<'a> {
    product: Datastore,
    collection: Option<&'a str>,
    operation: Option<&'a str>,
    host: Option<&'a str>,
    port_path_or_id: Option<&'a str>,
    database_name: Option<&'a str>,
    query: Option<&'a str>,
}

impl<'a> DatastoreParamsBuilder<'a> {
    /// Begin creating a new set of datastore parameters.
    pub fn new(product: Datastore) -> Self {
        DatastoreParamsBuilder {
            product,
            collection: None,
            operation: None,
            host: None,
            port_path_or_id: None,
            database_name: None,
            query: None,
        }
    }

    /// Set the table or collection being used or queried against.
    ///
    /// Must not contain any slash characters.
    pub fn collection(mut self, collection: &'a str) -> Self {
        self.collection = Some(collection);
        self
    }

    /// Set the operation being performed.
    ///
    /// For example, "select" for a SQL SELECT query, or "set" for
    /// a Memcached set operation. While operations may be specified
    /// with any case, New Relic suggests using lowercase.
    ///
    /// Must not contain any slash characters.
    pub fn operation(mut self, operation: &'a str) -> Self {
        self.operation = Some(operation);
        self
    }

    /// Set the datastore host name.
    ///
    /// Must not contain any slash characters.
    pub fn host(mut self, host: &'a str) -> Self {
        self.host = Some(host);
        self
    }

    /// Set the port or socket used to connect to the datastore.
    pub fn port_path_or_id(mut self, port_path_or_id: &'a str) -> Self {
        self.port_path_or_id = Some(port_path_or_id);
        self
    }

    /// Set the database name or number in use.
    pub fn database_name(mut self, database_name: &'a str) -> Self {
        self.database_name = Some(database_name);
        self
    }

    /// Set the database query that was sent to the datastore.
    ///
    /// For security reasons, this value is only used if you set the
    /// `product` to a supported sql-like datastore (`Datastore::Firebird`,
    /// `Datastore::MySQL`, `Datastore::Postgres`, etc.) This allows the SDK
    /// to correctly obfuscate the query. When the product is set otherwise,
    /// no query information is reported to New Relic.
    pub fn query(mut self, query: &'a str) -> Self {
        self.query = Some(query);
        self
    }

    /// Consume the builder, returning the set of datastore parameters.
    ///
    /// This will fail if any of the parameters contain null bytes.
    pub fn build(self) -> Result<DatastoreParams> {
        Ok(DatastoreParams {
            product: self.product.inner(),
            collection: cstring_or_null_ptr!(self.collection),
            operation: cstring_or_null_ptr!(self.operation),
            host: cstring_or_null_ptr!(self.host),
            port_path_or_id: cstring_or_null_ptr!(self.port_path_or_id),
            database_name: cstring_or_null_ptr!(self.database_name),
            query: cstring_or_null_ptr!(self.query),
        })
    }
}

/// Parameters used to instrument datastore segments.
///
/// Create this using `DatastoreParamsBuilder`.
pub struct DatastoreParams {
    product: *mut c_char,
    collection: *mut c_char,
    operation: *mut c_char,
    host: *mut c_char,
    port_path_or_id: *mut c_char,
    database_name: *mut c_char,
    query: *mut c_char,
}

impl DatastoreParams {
    fn as_ptr(&self) -> ffi::newrelic_datastore_segment_params_t {
        ffi::newrelic_datastore_segment_params_t {
            product: self.product,
            collection: self.collection,
            operation: self.operation,
            host: self.host,
            port_path_or_id: self.port_path_or_id,
            database_name: self.database_name,
            query: self.query,
        }
    }
}

impl Drop for DatastoreParams {
    fn drop(&mut self) {
        debug!("Reclaiming DatastoreParams");
        unsafe {
            drop_if_non_null!(self.collection);
            drop_if_non_null!(self.operation);
            drop_if_non_null!(self.host);
            drop_if_non_null!(self.port_path_or_id);
            drop_if_non_null!(self.database_name);
            drop_if_non_null!(self.query);
        }
    }
}

impl AsRef<Self> for DatastoreParams {
    fn as_ref(&self) -> &Self {
        self
    }
}

unsafe impl Send for DatastoreParams {}

unsafe impl Sync for DatastoreParams {}
