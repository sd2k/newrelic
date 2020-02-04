use std::{future::Future, pin::Pin, task::Context, task::Poll};

use pin_project::pin_project;

use crate::{segment, transaction::Transaction};

/// A trait to make a lifetime scoped reference to a transaction optional
pub trait OptionalTransaction<'a> {
    /// Return an optional transaction
    fn get_transaction(&'a self) -> Option<&'a Transaction>;
}

impl<'a> OptionalTransaction<'a> for &'a Transaction {
    fn get_transaction(&'a self) -> Option<&'a Transaction> {
        Some(self)
    }
}

impl<'a> OptionalTransaction<'a> for Option<&'a Transaction> {
    fn get_transaction(&'a self) -> Option<&'a Transaction> {
        *self
    }
}

/// Extension trait allowing a `std::future::Future` to be instrumented inside a `Segment`
pub trait Segmented: Sized {
    /// Instruments this future inside a custom `Segment`
    fn custom_segment<'a, T>(
        self,
        to_trans: &'a T,
        name: &str,
        category: &str,
    ) -> SegmentedFuture<'a, Self>
    where
        T: OptionalTransaction<'a>,
    {
        match to_trans.get_transaction() {
            Some(transaction) => SegmentedFuture {
                inner: self,
                segment: Some(segment::Segment::custom(transaction, name, category)),
            },
            None => SegmentedFuture {
                inner: self,
                segment: None,
            },
        }
    }

    /// Instruments this future inside a datastore `Segment`
    fn datastore_segment<'a, T>(
        self,
        to_trans: &'a T,
        params: &segment::DatastoreParams,
    ) -> SegmentedFuture<'a, Self>
    where
        T: OptionalTransaction<'a>,
    {
        match to_trans.get_transaction() {
            Some(transaction) => SegmentedFuture {
                inner: self,
                segment: Some(segment::Segment::datastore(transaction, params)),
            },
            None => SegmentedFuture {
                inner: self,
                segment: None,
            },
        }
    }

    /// Instruments this future inside an external `Segment`
    fn external_segment<'a, T>(
        self,
        to_trans: &'a T,
        params: &segment::ExternalParams,
    ) -> SegmentedFuture<'a, Self>
    where
        T: OptionalTransaction<'a>,
    {
        match to_trans.get_transaction() {
            Some(transaction) => SegmentedFuture {
                inner: self,
                segment: Some(segment::Segment::external(transaction, params)),
            },
            None => SegmentedFuture {
                inner: self,
                segment: None,
            },
        }
    }
}

impl<T: Sized> Segmented for T {}

/// A future that has been instrumented inside a `Segment`
#[pin_project]
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
