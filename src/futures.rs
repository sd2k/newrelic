use std::{future::Future, pin::Pin, task::Context, task::Poll};

use pin_project::pin_project;

use crate::{segment, transaction::Transaction};

/// A trait to make a lifetime scoped reference to a `Transaction` optional
///
/// Example:
///
/// ```rust
/// # use newrelic::Error;
/// # async fn run() -> Result<(), Error> {
/// use newrelic::{App, ExternalParamsBuilder, Segmented, Transaction};
///
/// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
///
/// let app = App::new("my app", &license_key).expect("Could not create app");
///
/// let transaction = app
///     .web_transaction("Transaction name")
///     .expect("Could not start transaction");
///
/// let possibly_a_transaction: Option<&Transaction> = Some(&transaction);
///
/// let not_a_transaction: Option<&Transaction> = None;
///
/// async { }
///     .custom_segment(&transaction, "Segment name", "Segment category")
///     .await;
///
/// async { }
///     .custom_segment(&possibly_a_transaction, "Segment name", "Segment category")
///     .await;
///
/// async { }
///     .custom_segment(&not_a_transaction, "Segment name", "Segment category")
///     .await;
///
/// # Ok(())
/// # }
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
pub trait OptionalTransaction<'a> {
    /// Return an optional transaction
    fn get_transaction(&'a self) -> Option<&'a Transaction>;
}

#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
impl<'a> OptionalTransaction<'a> for Transaction {
    fn get_transaction(&'a self) -> Option<&'a Transaction> {
        Some(self)
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
impl<'a> OptionalTransaction<'a> for Option<&'a Transaction> {
    fn get_transaction(&'a self) -> Option<&'a Transaction> {
        *self
    }
}

/// Extension trait allowing a `Future` to be instrumented inside a `Segment`
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
pub trait Segmented: Sized {
    /// Instruments this future inside a custom `Segment`
    ///
    /// Example:
    ///
    /// ```rust
    /// # use newrelic::Error;
    /// # async fn run() -> Result<(), Error> {
    /// use newrelic::{App, Segmented};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key).expect("Could not create app");
    ///
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    ///
    /// async { }
    ///     .custom_segment(&transaction, "Segment name", "Segment category")
    ///     .await;
    ///
    /// # Ok(())
    /// # }
    /// ```
    fn custom_segment<'a, T>(
        self,
        to_trans: &'a T,
        name: &str,
        category: &str,
    ) -> SegmentedFuture<'a, Self>
    where
        T: OptionalTransaction<'a>,
    {
        SegmentedFuture {
            inner: self,
            segment: to_trans
                .get_transaction()
                .map(|transaction| segment::Segment::custom(transaction, name, category)),
        }
    }

    /// Instruments this future inside a datastore `Segment`
    ///
    /// Example:
    ///
    /// ```rust
    /// # use newrelic::Error;
    /// # async fn run() -> Result<(), Error> {
    /// use newrelic::{App, Datastore, DatastoreParamsBuilder, Segmented};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key).expect("Could not create app");
    ///
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    ///
    /// async { }
    ///     .datastore_segment(
    ///         &transaction,
    ///         &DatastoreParamsBuilder::new(Datastore::Postgres)
    ///             .collection("people")
    ///             .operation("select")
    ///             .build()?
    ///     )
    ///     .await;
    ///
    /// # Ok(())
    /// # }
    /// ```
    fn datastore_segment<'a, T>(
        self,
        to_trans: &'a T,
        params: &segment::DatastoreParams,
    ) -> SegmentedFuture<'a, Self>
    where
        T: OptionalTransaction<'a>,
    {
        SegmentedFuture {
            inner: self,
            segment: to_trans
                .get_transaction()
                .map(|transaction| segment::Segment::datastore(transaction, params)),
        }
    }

    /// Instruments this future inside an external `Segment`
    ///
    /// Example:
    ///
    /// ```rust
    /// # use newrelic::Error;
    /// # async fn run() -> Result<(), Error> {
    /// use newrelic::{App, ExternalParamsBuilder, Segmented};
    ///
    /// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY").unwrap();
    ///
    /// let app = App::new("my app", &license_key).expect("Could not create app");
    ///
    /// let transaction = app
    ///     .web_transaction("Transaction name")
    ///     .expect("Could not start transaction");
    ///
    /// async { }
    ///     .external_segment(
    ///         &transaction,
    ///         &ExternalParamsBuilder::new("https://www.rust-lang.org/")
    ///             .procedure("GET")
    ///             .library("reqwest")
    ///             .build()?
    ///     )
    ///     .await;
    ///
    /// # Ok(())
    /// # }
    /// ```
    fn external_segment<'a, T>(
        self,
        to_trans: &'a T,
        params: &segment::ExternalParams,
    ) -> SegmentedFuture<'a, Self>
    where
        T: OptionalTransaction<'a>,
    {
        SegmentedFuture {
            inner: self,
            segment: to_trans
                .get_transaction()
                .map(|transaction| segment::Segment::external(transaction, params)),
        }
    }
}

impl<T: Sized> Segmented for T {}

/// A future that has been instrumented inside a `Segment`
#[pin_project]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
pub struct SegmentedFuture<'a, T> {
    #[pin]
    inner: T,

    segment: Option<segment::Segment<'a>>,
}

impl<'a, T: Future> Future for SegmentedFuture<'a, T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result = this.inner.poll(cx);

        if result.is_ready() {
            // Drop the segment
            *this.segment = None;
        }

        result
    }
}
