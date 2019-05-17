use std::ffi::CString;

use log::{debug, error};
use newrelic_sys as ffi;

use crate::{error::Result, transaction::Transaction};

/// The actual details of a segment. See `Segment` for the user facing API;
/// this struct is just used internally behind an `Option`.
struct InnerSegment<'a> {
    transaction: &'a Transaction,
    inner: *mut ffi::newrelic_segment_t,
}

/// A segment within a transaction.
///
/// Use segments to instrument transactions with greater granularity.
/// Segments are created using the various methods on a `Transaction`.
///
/// Segments can be nested by calling the various `_nested` methods on
/// an existing segment.
pub struct Segment<'a> {
    /// This holds either the actual segment, if creation was successful,
    /// or None, if creation failed.
    /// This means users don't have to deal with Results and segment
    /// creation can fail quietly. Usually this would be bad, but we probably
    /// just want to continue even if New Relic monitoring isn't working...
    /// right?
    inner: Option<InnerSegment<'a>>,
}

impl<'a> Segment<'a> {
    pub(crate) fn custom(transaction: &'a Transaction, name: &str, category: &str) -> Self {
        let c_name = CString::new(name);
        let c_category = CString::new(category);

        let inner = match (c_name, c_category) {
            (Ok(c_name), Ok(c_category)) => {
                let inner = unsafe {
                    ffi::newrelic_start_segment(
                        transaction.inner,
                        c_name.as_ptr(),
                        c_category.as_ptr(),
                    )
                };
                if inner.is_null() {
                    error!(
                        "Could not create segment with name {} due to invalid transaction",
                        name
                    );
                    None
                } else {
                    Some(InnerSegment { transaction, inner })
                }
            }
            _ => {
                error!(
                    "Could not create segment with name {}, category {}, due to NUL string in name or category",
                    name,
                    category,
                );
                None
            }
        };
        debug!("Created segment");
        Segment { inner }
    }

    pub(crate) fn datastore(transaction: &'a Transaction, params: DatastoreParams) -> Self {
        let inner_ptr =
            unsafe { ffi::newrelic_start_datastore_segment(transaction.inner, &params.as_ptr()) };
        let inner = if inner_ptr.is_null() {
            error!("Could not create datastore segment due to invalid transaction");
            None
        } else {
            Some(InnerSegment {
                transaction,
                inner: inner_ptr,
            })
        };
        debug!("Created segment");
        Segment { inner }
    }

    pub(crate) fn external(transaction: &'a Transaction, params: ExternalParams) -> Self {
        debug!("Trying to start external segment");
        let inner_ptr =
            unsafe { ffi::newrelic_start_external_segment(transaction.inner, &params.as_ptr()) };
        let inner = if inner_ptr.is_null() {
            error!("Could not create external segment due to invalid transaction");
            None
        } else {
            Some(InnerSegment {
                transaction,
                inner: inner_ptr,
            })
        };
        debug!("Created segment");
        Segment { inner }
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
    /// # if false {
    /// let app = App::new("Test app", "Test license key")
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
    /// # }
    /// ```
    pub fn custom_nested<F, V>(&self, name: &str, category: &str, func: F) -> V
    where
        F: FnOnce(Segment) -> V,
    {
        // We can only create a nested segment if this segment is 'real'
        if let Some(inner) = &self.inner {
            let nested_segment = Segment::custom(inner.transaction, name, category);

            // Only try and set the segment parent if creation succeeded
            if let Some(segment) = &nested_segment.inner {
                unsafe {
                    ffi::newrelic_set_segment_parent(segment.inner, inner.inner);
                }
            }
            func(nested_segment)
        } else {
            func(Segment { inner: None })
        }
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
    /// # if false {
    /// let app = App::new("Test app", "Test license key")
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
    ///     let expensive_val = s.datastore_nested(datastore_segment_params, |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         3
    ///     });
    ///     expensive_val * 2
    /// });
    /// # }
    /// ```
    pub fn datastore_nested<F, V>(&self, params: DatastoreParams, func: F) -> V
    where
        F: FnOnce(Segment) -> V,
    {
        // We can only create a nested segment if this segment is 'real'
        if let Some(inner) = &self.inner {
            let nested_segment = Segment::datastore(inner.transaction, params);

            // Only try and set the segment parent if creation succeeded
            if let Some(segment) = &nested_segment.inner {
                unsafe {
                    ffi::newrelic_set_segment_parent(segment.inner, inner.inner);
                }
            }
            func(nested_segment)
        } else {
            func(Segment { inner: None })
        }
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
    /// # if false {
    /// let app = App::new("Test app", "Test license key")
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
    ///     let expensive_val = s.external_nested(external_segment_params, |_| {
    ///         thread::sleep(Duration::from_secs(1));
    ///         3
    ///     });
    ///     expensive_val * 2
    /// });
    /// # }
    /// ```
    pub fn external_nested<F, V>(&self, params: ExternalParams, func: F) -> V
    where
        F: FnOnce(Segment) -> V,
    {
        // We can only create a nested segment if this segment is 'real'
        if let Some(inner) = &self.inner {
            let nested_segment = Segment::external(inner.transaction, params);

            // Only try and set the segment parent if creation succeeded
            if let Some(segment) = &nested_segment.inner {
                unsafe {
                    ffi::newrelic_set_segment_parent(segment.inner, inner.inner);
                }
            }
            func(nested_segment)
        } else {
            func(Segment { inner: None })
        }
    }
}

impl<'a> Drop for Segment<'a> {
    fn drop(&mut self) {
        if let Some(ref mut inner) = self.inner {
            unsafe {
                ffi::newrelic_end_segment(inner.transaction.inner, &mut inner.inner);
            }
            debug!("Ended segment");
        }
        self.inner = None;
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
    uri: *mut i8,
    procedure: *mut i8,
    library: *mut i8,
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
    fn inner(&self) -> *mut i8 {
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
        datastore as *mut i8
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
    product: *mut i8,
    collection: *mut i8,
    operation: *mut i8,
    host: *mut i8,
    port_path_or_id: *mut i8,
    database_name: *mut i8,
    query: *mut i8,
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
