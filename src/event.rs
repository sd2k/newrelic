use std::ffi::CString;

use log::{debug, error, warn};
use newrelic_sys as ffi;

use crate::{
    error::{Error, Result},
    transaction::{Attribute, Transaction},
};

/// A custom event to be added to a transaction.
#[must_use]
pub struct CustomEvent<'a> {
    transaction: &'a Transaction,
    inner: *mut ffi::newrelic_custom_event_t,
    recorded: bool,
}

impl<'a> CustomEvent<'a> {
    pub(crate) fn new(transaction: &'a Transaction, event_type: &str) -> Result<Self> {
        let event_type = CString::new(event_type)?;
        let inner = unsafe { ffi::newrelic_create_custom_event(event_type.as_ptr()) };
        debug!("Created custom event");
        Ok(CustomEvent {
            inner,
            transaction,
            recorded: false,
        })
    }

    /// Add an attribute to this custom event.
    pub fn add_attribute(&self, name: &str, attribute: &Attribute) -> Result<&Self> {
        let name = CString::new(name)?;
        let ok = match attribute {
            Attribute::Int(i) => unsafe {
                ffi::newrelic_custom_event_add_attribute_int(self.inner, name.as_ptr(), *i)
            },
            Attribute::Float(f) => unsafe {
                ffi::newrelic_custom_event_add_attribute_double(self.inner, name.as_ptr(), *f)
            },
            Attribute::Long(l) => unsafe {
                ffi::newrelic_custom_event_add_attribute_long(self.inner, name.as_ptr(), *l)
            },
            Attribute::String(s) => {
                let s = CString::new(*s)?;
                unsafe {
                    ffi::newrelic_custom_event_add_attribute_string(
                        self.inner,
                        name.as_ptr(),
                        s.as_ptr(),
                    )
                }
            }
        };
        if ok {
            debug!("Added attribute to custom event");
            Ok(self)
        } else {
            error!("Could not add custom attribute");
            Err(Error::AttributeError)
        }
    }

    /// Record this custom event, consuming it.
    pub fn record(mut self) {
        unsafe { ffi::newrelic_record_custom_event(self.transaction.inner, &mut self.inner) };
        debug!("Recorded custom event");
        self.recorded = true;
    }
}

impl<'a> Drop for CustomEvent<'a> {
    /// If the custom event wasn't ever recorded, we should discard it
    /// to free the memory.
    fn drop(&mut self) {
        if !self.recorded {
            warn!("Dropping unrecorded custom event");
            unsafe { ffi::newrelic_discard_custom_event(&mut self.inner) };
        }
    }
}
