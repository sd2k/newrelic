use std::{ffi::CString, time::Duration};

use log::{debug, error};
use newrelic_sys as ffi;

use crate::{
    app::App,
    error::{Error, Result},
    event::CustomEvent,
    segment::{DatastoreParams, ExternalParams, Segment},
};

/// A type of transaction monitored by New Relic.
pub enum TransactionType {
    /// A web transaction.
    Web,
    /// A non-web transaction.
    NonWeb,
}

/// An attribute to add to a transaction.
#[derive(Debug)]
pub enum Attribute<'a> {
    /// A short (i32) integer attribute.
    Int(i32),
    /// A long (i64) integer attribute.
    Long(i64),
    /// A float (f64) attribute.
    Float(f64),
    /// A string attribute.
    String(&'a str),
    /// An owned string attribute.
    OwnedString(&'a String),
}

impl<'a> From<i32> for Attribute<'a> {
    #[allow(unused_variables)]
    #[inline]
    fn from(original: i32) -> Attribute<'a> {
        Attribute::Int(original)
    }
}
impl<'a> From<i64> for Attribute<'a> {
    #[allow(unused_variables)]
    #[inline]
    fn from(original: i64) -> Attribute<'a> {
        Attribute::Long(original)
    }
}
impl<'a> From<f64> for Attribute<'a> {
    #[allow(unused_variables)]
    #[inline]
    fn from(original: f64) -> Attribute<'a> {
        Attribute::Float(original)
    }
}
impl<'a> From<&'a str> for Attribute<'a> {
    #[allow(unused_variables)]
    #[inline]
    fn from(original: &'a str) -> Attribute<'a> {
        Attribute::String(original)
    }
}
impl<'a> From<&'a String> for Attribute<'a> {
    #[allow(unused_variables)]
    #[inline]
    fn from(original: &'a String) -> Attribute<'a> {
        Attribute::OwnedString(original)
    }
}

#[derive(PartialEq)]
enum State {
    Running,
    Ended,
}

/// A transaction monitored by New Relic.
pub struct Transaction {
    pub(crate) inner: *mut ffi::newrelic_txn_t,
    _type: TransactionType,
    state: State,
}

impl Transaction {
    pub(crate) fn web(app: &App, name: &str) -> Result<Self> {
        let name = CString::new(name)?;
        let inner = unsafe { ffi::newrelic_start_web_transaction(app.inner, name.as_ptr()) };
        if inner.is_null() {
            error!("Could not start web transaction");
            Err(Error::TransactionStartError)
        } else {
            debug!("Started web transaction");
            Ok(Transaction {
                inner,
                _type: TransactionType::Web,
                state: State::Running,
            })
        }
    }

    pub(crate) fn non_web(app: &App, name: &str) -> Result<Self> {
        let name = CString::new(name)?;
        let inner = unsafe { ffi::newrelic_start_non_web_transaction(app.inner, name.as_ptr()) };
        if inner.is_null() {
            error!("Could not start non-web transaction");
            Err(Error::TransactionStartError)
        } else {
            debug!("Started non-web transaction");
            Ok(Transaction {
                inner,
                _type: TransactionType::NonWeb,
                state: State::Running,
            })
        }
    }

    /// Get the type of the transaction.
    pub fn r#type(&self) -> &TransactionType {
        &self._type
    }

    /// Add an attribute to the transaction.
    ///
    /// Returns an error if the New Relic SDK returns an error.
    pub fn add_attribute<'a, T>(&self, name: &str, attribute: T) -> Result<()>
    where
        T: Into<Attribute<'a>>,
    {
        let name = CString::new(name)?;
        let ok = match attribute.into() {
            Attribute::Int(i) => unsafe {
                ffi::newrelic_add_attribute_int(self.inner, name.as_ptr(), i)
            },
            Attribute::Float(f) => unsafe {
                ffi::newrelic_add_attribute_double(self.inner, name.as_ptr(), f)
            },
            Attribute::Long(l) => unsafe {
                ffi::newrelic_add_attribute_long(self.inner, name.as_ptr(), l)
            },
            Attribute::String(s) => {
                let s = CString::new(s)?;
                unsafe { ffi::newrelic_add_attribute_string(self.inner, name.as_ptr(), s.as_ptr()) }
            }
            Attribute::OwnedString(s) => {
                let s = CString::new(s.as_str())?;
                unsafe { ffi::newrelic_add_attribute_string(self.inner, name.as_ptr(), s.as_ptr()) }
            }
        };
        if ok {
            Ok(())
        } else {
            Err(Error::AttributeError)
        }
    }

    /// Create a custom segment within this transaction.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::App;
    ///
    /// # if false {
    /// let app = App::new("Test app", "Test license key")
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Test transaction")
    ///     .expect("Could not start transaction");
    /// transaction.custom_segment("Test segment", "Test category", |_| {
    ///     thread::sleep(Duration::from_secs(1))
    /// });
    /// # }
    /// ```
    pub fn custom_segment<F, V>(&self, name: &str, category: &str, func: F) -> V
    where
        F: FnOnce(Segment) -> V,
    {
        let segment = Segment::custom(self, name, category);
        func(segment)
    }

    /// Create a datastore segment within this transaction.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, Datastore, DatastoreParamsBuilder};
    ///
    /// # if false {
    /// let app = App::new("Test app", "Test license key")
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Test transaction")
    ///     .expect("Could not start transaction");
    /// let segment_params = DatastoreParamsBuilder::new(Datastore::Postgres)
    ///     .collection("people")
    ///     .operation("select")
    ///     .build()
    ///     .expect("Invalid datastore segment parameters");
    /// transaction.datastore_segment(&segment_params, |_| {
    ///     thread::sleep(Duration::from_secs(1))
    /// });
    /// # }
    /// ```
    pub fn datastore_segment<F, V>(&self, params: &DatastoreParams, func: F) -> V
    where
        F: FnOnce(Segment) -> V,
    {
        let segment = Segment::datastore(self, params);
        func(segment)
    }

    /// Create an external segment within this transaction.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::{App, ExternalParamsBuilder};
    ///
    /// # if false {
    /// let app = App::new("Test app", "Test license key")
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Test transaction")
    ///     .expect("Could not start transaction");
    /// let segment_params = ExternalParamsBuilder::new("https://www.rust-lang.org/")
    ///     .procedure("GET")
    ///     .library("reqwest")
    ///     .build()
    ///     .expect("Invalid external segment parameters");
    /// transaction.external_segment(&segment_params, |_| {
    ///     thread::sleep(Duration::from_secs(1))
    /// });
    /// # }
    /// ```
    pub fn external_segment<F, V>(&self, params: &ExternalParams, func: F) -> V
    where
        F: FnOnce(Segment) -> V,
    {
        let segment = Segment::external(self, params);
        func(segment)
    }

    /// Record an error in this transaction.
    ///
    /// `priority` is an arbitrary integer indicating the error priority.
    /// `message` is the error message; `class` is the error class or type.
    pub fn notice_error(&self, priority: i32, message: &str, class: &str) -> Result<()> {
        let message = CString::new(message)?;
        let class = CString::new(class)?;
        unsafe {
            ffi::newrelic_notice_error(self.inner, priority, message.as_ptr(), class.as_ptr());
        }
        Ok(())
    }

    /// Ignore this transaction.
    ///
    /// Data for this transaction will not be sent to New Relic.
    pub fn ignore(&self) -> Result<()> {
        let ok = unsafe { ffi::newrelic_ignore_transaction(self.inner) };
        if ok {
            Ok(())
        } else {
            Err(Error::IgnoreError)
        }
    }

    /// Record a custom metric for this transaction.
    ///
    /// The metric will be named according to `metric_name` and will
    /// record for `duration`.
    pub fn record_custom_metric(&self, metric_name: &str, duration: Duration) -> Result<()> {
        let metric_name = CString::new(metric_name)?;
        let ok = unsafe {
            ffi::newrelic_record_custom_metric(
                self.inner,
                metric_name.as_ptr(),
                duration.as_millis() as f64,
            )
        };
        if ok {
            Ok(())
        } else {
            Err(Error::CustomMetricError)
        }
    }

    /// Create a custom event attached to this transaction.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::{thread, time::Duration};
    ///
    /// use newrelic::App;
    ///
    /// # if false {
    /// let app = App::new("Test app", "Test license key")
    ///     .expect("Could not create app");
    /// let transaction = app
    ///     .web_transaction("Test transaction")
    ///     .expect("Could not start transaction");
    /// let custom_event = transaction.custom_event("My event")
    ///     .expect("Could not create custom event");
    /// custom_event.add_attribute("number of foos", 1_000);
    /// custom_event.record();
    /// # }
    /// ```
    pub fn custom_event(&self, event_type: &str) -> Result<CustomEvent> {
        CustomEvent::new(self, event_type)
    }

    /// Explicitly end this transaction.
    ///
    /// If this is not called, the transaction is automatically ended
    /// when dropped.
    pub fn end(&mut self) {
        if let State::Running = self.state {
            unsafe {
                ffi::newrelic_end_transaction(&mut self.inner);
            }
            debug!("Ended transaction");
            self.state = State::Ended;
        }
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        self.end();
    }
}

unsafe impl Send for Transaction {}
unsafe impl Sync for Transaction {}
