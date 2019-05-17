use std::ffi::NulError;
use std::fmt;

#[derive(Debug)]
/// An error caused by the New Relic SDK.
///
/// The error message is provided by this library since
/// the SDK doesn't pass error messages back to callers.
/// Configure the SDK log level / log output using `NewRelicConfig`
/// for greater detail.
pub enum Error {
    /// There was an error setting a transaction attribute.
    AttributeError,
    /// There was an error configuring the New Relic app.
    ///
    /// This is likely due to an invalid license key; check the New Relic SDK
    /// logs for more details.
    ConfigError,
    /// The custom metric could not be created.
    CustomMetricError,
    /// There was an error connecting to the New Relic daemon.
    ///
    /// Be sure to read the official New Relic documentation on the
    /// [architecture of the C SDK](https://docs.newrelic.com/docs/agents/c-sdk/get-started/introduction-c-sdk#architecture).
    ///
    /// If errors still occur after checking the daemon setup, check the
    /// New Relic SDK logs for more details.
    DaemonError,
    /// The transaction could not be ignored.
    IgnoreError,
    /// The provided log file contained non-unicode characters.
    LogFileError,
    /// The New Relic SDK returned an error when attempting to configure
    /// logging. Check the SDK logs for more details.
    LoggingError,
    /// The transaction could not be started.
    /// Check the New Relic SDK logs for more details.
    TransactionStartError,
    /// A string parameter contained a null byte and could not be converted
    /// to a CString.
    NulError(NulError),
}

impl From<NulError> for Error {
    fn from(error: NulError) -> Self {
        Error::NulError(error)
    }
}

const CHECK_NEW_RELIC_LOGS: &str = "check New Relic logs for details";

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::AttributeError => write!(f, "Error setting attribute; {}", CHECK_NEW_RELIC_LOGS),
            Error::ConfigError => write!(
                f,
                "Error configuring New Relic app; {}",
                CHECK_NEW_RELIC_LOGS
            ),
            Error::DaemonError => write!(
                f,
                "Error connecting to New Relic daemon; {}",
                CHECK_NEW_RELIC_LOGS
            ),
            Error::CustomMetricError => {
                write!(f, "Error recording custom metric; {}", CHECK_NEW_RELIC_LOGS)
            }
            Error::IgnoreError => write!(f, "Error ignoring transaction; {}", CHECK_NEW_RELIC_LOGS),
            Error::NulError(inner) => write!(f, "{}", inner),
            Error::LogFileError => write!(f, "Invalid log file (must be valid Unicode)"),
            Error::LoggingError => write!(f, "Error configuring logging; {}", CHECK_NEW_RELIC_LOGS),
            Error::TransactionStartError => {
                write!(f, "Error starting transaction; {}", CHECK_NEW_RELIC_LOGS)
            }
        }
    }
}

/// A Result used by the New Relic library.
pub type Result<T> = std::result::Result<T, Error>;
