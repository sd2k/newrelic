use std::{convert::TryFrom, ffi::CString, path::Path, time::Duration};

use log::{self, debug};
use newrelic_sys as ffi;

use crate::{
    error::{Error, Result},
    transaction::Transaction,
};

/// The default timeout when connecting to the daemon upon app creation.
pub const DEFAULT_APP_TIMEOUT: u16 = 10000;

/// Whether to consider transactions for trace generation based on the apdex configuration or a
/// specific duration.
pub enum TracingThreshold {
    /// Use 4*apdex(T) as the minimum time a transaction must take before  a trace may be generated
    ApdexFailing,
    /// The minimum transaction time before a trace may be generated, in microseconds
    OverDuration(Duration),
}

/// Controls the format of the sql put into transaction traces for supported sql-like products.
pub enum RecordSQL {
    /// Transaction traces have no sql in them.
    Off,
    /// The sql is added to the transaction trace as-is.
    Raw,
    /// Alphanumeric characters are set to '?'. For example 'SELECT * FROM table WHERE foo = 42'
    /// is reported as 'SELECT * FROM table WHERE foo = ?'. These obfuscated queries are added to
    /// the transaction trace for supported datastore products.
    Obfuscated,
}

/// A builder to construct a New Relic application
///
/// Example:
///
/// ```rust
/// use std::time::Duration;
///
/// use newrelic::{AppBuilder, TracingThreshold};
///
/// # fn main() -> Result<(), newrelic::Error> {
/// let license_key = std::env::var("NEW_RELIC_LICENSE_KEY")
///     .unwrap_or_else(|_| "example-license-key".to_string());
/// let app = AppBuilder::new("my app", &license_key)
///     .expect("Invalid license key or app name")
///     .transaction_threshold(
///         TracingThreshold::OverDuration(Duration::from_millis(100))
///     )?
///     .span_events(true)
///     .build()
///     .expect("Unable to create app");
/// # Ok(())
/// # }
/// ```
pub struct AppBuilder {
    config: AppConfig,
}

impl AppBuilder {
    /// Begin creating an App
    pub fn new(name: &str, license_key: &str) -> Result<Self> {
        Ok(Self {
            config: AppConfig::new(name, license_key)?,
        })
    }

    /// Whether to enable transaction traces.
    ///
    /// If set to true for a transaction, the transaction tracer records the top-10 slowest queries
    /// along with a stack trace of where the call occurred.
    pub fn transaction_tracing(&mut self, enabled: bool) -> &mut Self {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();
        config.transaction_tracer.enabled = enabled;
        self
    }

    /// Whether to consider transactions for trace generation based on the apdex configuration or a
    /// specific duration.
    pub fn transaction_threshold(&mut self, threshold: TracingThreshold) -> Result<&mut Self> {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();

        match threshold {
            TracingThreshold::ApdexFailing => {
                config.transaction_tracer.threshold = ffi::_newrelic_transaction_tracer_threshold_t_NEWRELIC_THRESHOLD_IS_APDEX_FAILING;
            }
            TracingThreshold::OverDuration(duration) => {
                config.transaction_tracer.threshold = ffi::_newrelic_transaction_tracer_threshold_t_NEWRELIC_THRESHOLD_IS_OVER_DURATION;
                config.transaction_tracer.duration_us =
                    TryFrom::try_from(duration.as_micros()).map_err(|_| Error::DurationOverFlow)?;
            }
        };

        Ok(self)
    }

    /// Sets the threshold above which the New Relic SDK will record a stack trace for a
    /// transaction trace.
    pub fn stack_trace_threshold(&mut self, duration: Duration) -> Result<&mut Self> {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();
        config.transaction_tracer.stack_trace_threshold_us =
            TryFrom::try_from(duration.as_micros()).map_err(|_| Error::DurationOverFlow)?;
        Ok(self)
    }

    /// Whether slow datastore queries are recorded.
    pub fn datastore_reporting(&mut self, enabled: bool) -> &mut Self {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();
        config.transaction_tracer.datastore_reporting.enabled = enabled;
        self
    }

    /// Specify the threshold above which a datastore query is considered "slow".
    pub fn datastore_reporting_threshold(&mut self, duration: Duration) -> Result<&mut Self> {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();
        config.transaction_tracer.datastore_reporting.threshold_us =
            TryFrom::try_from(duration.as_micros()).map_err(|_| Error::DurationOverFlow)?;
        Ok(self)
    }

    /// Controls the format of the sql put into transaction traces for supported sql-like products.
    ///
    /// Only relevant if datastore_reporting is enabled
    pub fn record_sql(&mut self, record_sql: RecordSQL) -> &mut Self {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();
        config.transaction_tracer.datastore_reporting.record_sql = match record_sql {
            RecordSQL::Off => ffi::_newrelic_tt_recordsql_t_NEWRELIC_SQL_OFF,
            RecordSQL::Raw => ffi::_newrelic_tt_recordsql_t_NEWRELIC_SQL_RAW,
            RecordSQL::Obfuscated => ffi::_newrelic_tt_recordsql_t_NEWRELIC_SQL_OBFUSCATED,
        };
        self
    }

    /// Whether database names inside datastore segments are reported to New Relic.
    pub fn database_name_reporting(&mut self, enabled: bool) -> &mut Self {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();
        config.datastore_tracer.database_name_reporting = enabled;
        self
    }

    /// Whether host and port inside datastore segments are reported to New Relic.
    pub fn datastore_instance_reporting(&mut self, enabled: bool) -> &mut Self {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();
        config.datastore_tracer.instance_reporting = enabled;
        self
    }

    /// Whether or not span events are generated.
    pub fn span_events(&mut self, enabled: bool) -> &mut Self {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();
        config.span_events.enabled = enabled;
        self
    }

    /// Whether to enable distributed tracing.
    #[cfg(feature = "distributed_tracing")]
    #[cfg_attr(docsrs, doc(cfg(feature = "distributed_tracing")))]
    pub fn distributed_tracing(&mut self, enabled: bool) -> &mut Self {
        let config = unsafe { self.config.inner.as_mut() }.unwrap();
        config.distributed_tracing.enabled = enabled;
        self
    }

    /// Consume the builder, returning the `App`.
    pub fn build(&self) -> Result<App> {
        App::with_timeout_ref(&self.config, DEFAULT_APP_TIMEOUT)
    }
}

#[must_use = "must be used by an App"]
/// Application config used by New Relic.
pub struct AppConfig {
    inner: *mut ffi::_newrelic_app_config_t,
}

impl AppConfig {
    /// Create a new `AppConfig` with the given application name
    /// and license key.
    ///
    /// This function may return `Err` if the name or license key contain
    /// a NUL byte, or if the SDK deems the name or license key unsuitable.
    pub fn new(name: &str, license_key: &str) -> Result<Self> {
        let name = CString::new(name)?;
        let license_key = CString::new(license_key)?;
        let inner = unsafe { ffi::newrelic_create_app_config(name.as_ptr(), license_key.as_ptr()) };
        if inner.is_null() {
            Err(Error::ConfigError)
        } else {
            Ok(AppConfig { inner })
        }
    }
}

impl Drop for AppConfig {
    fn drop(&mut self) {
        unsafe {
            ffi::newrelic_destroy_app_config(&mut self.inner);
        }
    }
}

/// A New Relic application.
pub struct App {
    pub(crate) inner: *mut ffi::newrelic_app_t,
}

impl App {
    /// Create a new application.
    ///
    /// Uses the default timeout, `DEFAULT_APP_TIMEOUT`, when establishing a
    /// connection to the daemon.
    pub fn new(name: &str, license_key: &str) -> Result<Self> {
        let config = AppConfig::new(name, license_key)?;
        App::with_timeout(config, DEFAULT_APP_TIMEOUT)
    }

    /// Create a new application using the specified config.
    ///
    /// Uses the default timeout, `DEFAULT_APP_TIMEOUT`, when establishing a
    /// connection to the daemon.
    pub fn with_config(config: AppConfig) -> Result<Self> {
        App::with_timeout(config, DEFAULT_APP_TIMEOUT)
    }

    /// Create a new application using the specified time as the maximum time
    /// to wait for a connection to the daemon to be established; a value of 0
    /// only makes one attempt at connecting to the daemon.
    pub fn with_timeout(config: AppConfig, timeout: u16) -> Result<Self> {
        Self::with_timeout_ref(&config, timeout)
    }

    fn with_timeout_ref(config: &AppConfig, timeout: u16) -> Result<Self> {
        let inner = unsafe { ffi::newrelic_create_app(config.inner, timeout) };
        if inner.is_null() {
            Err(Error::ConfigError)
        } else {
            debug!("Created app");
            Ok(App { inner })
        }
    }

    /// Begin a new web transaction in New Relic with the given name.
    ///
    /// This function will return an `Err` if the name contains a NUL byte.
    pub fn web_transaction(&self, name: &str) -> Result<Transaction> {
        Transaction::web(self, name)
    }

    /// Begin a new non-web transaction in New Relic with the given name.
    ///
    /// This function will return an `Err` if the name contains a NUL byte.
    pub fn non_web_transaction(&self, name: &str) -> Result<Transaction> {
        Transaction::non_web(self, name)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            ffi::newrelic_destroy_app(&mut self.inner);
        }
        debug!("Destroyed app");
    }
}

unsafe impl Send for App {}
unsafe impl Sync for App {}

/// The log level of the New Relic SDK.
enum LogLevel {
    /// The highest-priority log level; only errors are logged.
    Error,
    /// The log level for warnings and errors.
    Warning,
    /// The log level for informational logs, warnings, and errors.
    Info,
    /// The highest-verbosity log level.
    Debug,
}

impl LogLevel {
    fn inner(&self) -> ffi::_newrelic_loglevel_t {
        match self {
            LogLevel::Error => ffi::_newrelic_loglevel_t_NEWRELIC_LOG_ERROR,
            LogLevel::Warning => ffi::_newrelic_loglevel_t_NEWRELIC_LOG_WARNING,
            LogLevel::Info => ffi::_newrelic_loglevel_t_NEWRELIC_LOG_INFO,
            LogLevel::Debug => ffi::_newrelic_loglevel_t_NEWRELIC_LOG_DEBUG,
        }
    }
}

impl From<log::Level> for LogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => LogLevel::Error,
            log::Level::Warn => LogLevel::Warning,
            log::Level::Info => LogLevel::Info,
            log::Level::Debug => LogLevel::Debug,
            log::Level::Trace => LogLevel::Debug,
        }
    }
}

/// The output of the New Relic SDK logs.
pub enum LogOutput<'a> {
    /// Log to stderr.
    StdErr,
    /// Log to stdout.
    StdOut,
    /// Log to a file.
    File(&'a Path),
}

impl<'a> LogOutput<'a> {
    fn to_str(&self) -> Option<&'a str> {
        match self {
            LogOutput::StdErr => Some("stderr"),
            LogOutput::StdOut => Some("stdout"),
            LogOutput::File(path) => path.to_str(),
        }
    }
}

/// Custom configuration used to connect to the New Relic daemon.
///
/// This only needs to be used if the New Relic daemon is
/// running at a non-default location, or a different timeout is desired
///
/// Example:
///
/// ```rust
/// use std::time::Duration;
///
/// use newrelic::{NewRelicConfig, LogOutput, LogLevel};
///
/// # if false {
/// NewRelicConfig::default()
///     .socket("/tmp/newrelic-alternative.sock")
///     .timeout(Duration::from_millis(10000))
///     .init()
///     .expect("Could not connect to daemon!");
/// # }
/// ```
#[must_use]
pub struct NewRelicConfig<'a> {
    socket: Option<&'a str>,
    timeout: Option<Duration>,
    log_level: LogLevel,
    log_output: Option<LogOutput<'a>>,
}

impl<'a> Default for NewRelicConfig<'a> {
    fn default() -> Self {
        NewRelicConfig {
            socket: None,
            timeout: None,
            log_level: LogLevel::Info,
            log_output: None,
        }
    }
}

impl<'a> NewRelicConfig<'a> {
    /// Set the socket address used to connect to the New Relic daemon.
    ///
    /// Generally, this function only needs to be called explicitly
    /// if the daemon socket location needs to be customised. By default,
    /// "/tmp/.newrelic.sock" is used, which matches the default socket
    /// location used by newrelic-daemon if one isn't given.
    ///
    /// On Linux, if this starts with a literal '@', then this is treated
    /// as the name of an abstract domain socket instead of a filesystem path.
    ///
    /// Examples:
    ///
    /// ```rust
    /// use newrelic::NewRelicConfig;
    ///
    /// NewRelicConfig::default()
    ///     .socket("/tmp/.newrelic-alternative.sock")
    ///     .init();
    /// ```
    pub fn socket(mut self, socket: &'a str) -> Self {
        self.socket = Some(socket);
        self
    }

    /// Set the amount of time that the SDK will wait
    /// for a response from the daemon before considering initialization
    /// to have failed. If this is 0 or unset then the SDK's default value
    /// will be used.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::time::Duration;
    ///
    /// use newrelic::NewRelicConfig;
    ///
    /// NewRelicConfig::default()
    ///     .timeout(Duration::from_millis(10000))
    ///     .init();
    /// ```
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Configure logging for the New Relic SDK.
    ///
    /// Defaults to `LogLevel::Info` and `LogOutput::StdErr`.
    ///
    /// Note that this differs to the logs of the New Relic daemon,
    /// which are output by the daemon itself.
    ///
    /// Examples:
    ///
    /// Logging to stderr:
    ///
    /// ```rust
    /// use newrelic::{NewRelicConfig, LogLevel, LogOutput};
    ///
    /// NewRelicConfig::default()
    ///     .logging(LogLevel::Debug, LogOutput::StdErr)
    ///     .init();
    /// ```
    ///
    /// Logging to a file:
    ///
    /// ```rust
    /// use std::path::Path;
    ///
    /// use newrelic::{NewRelicConfig, LogLevel, LogOutput};
    ///
    /// # if false {
    /// NewRelicConfig::default()
    ///     .logging(LogLevel::Debug, LogOutput::File(Path::new("test.txt")))
    ///     .init();
    /// # }
    /// ```
    pub fn logging(mut self, level: log::Level, output: LogOutput<'a>) -> Self {
        self.log_output = Some(output);
        self.log_level = level.into();
        self
    }

    /// Initialise the New Relic SDK.
    ///
    /// If non-default settings are to be used, this must be called
    /// before the first `App` is created.
    ///
    /// Example:
    ///
    /// ```rust
    /// use std::path::Path;
    /// use newrelic::{NewRelicConfig, LogLevel, LogOutput};
    ///
    /// # if false {
    /// NewRelicConfig::default()
    ///     .logging(LogLevel::Info, LogOutput::File(Path::new("test.txt")))
    ///     .init();
    /// # }
    /// ```
    pub fn init(self) -> Result<()> {
        if let Some(log_output) = self.log_output {
            debug!("Configuring logging");
            let log_output = log_output.to_str().ok_or(Error::LogFileError)?;
            let log_output = CString::new(log_output)?;
            let logging_ok =
                unsafe { ffi::newrelic_configure_log(log_output.as_ptr(), self.log_level.inner()) };
            if !logging_ok {
                return Err(Error::LoggingError);
            }
        } else {
            debug!("Not configuring logging");
        }
        let socket = match self.socket {
            Some(s) => Some(CString::new(s)?),
            None => None,
        };
        let timeout = self.timeout.map(|t| t.as_millis()).unwrap_or(0) as i32;
        let socket = socket
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or_else(std::ptr::null);
        let ok = unsafe { ffi::newrelic_init(socket, timeout) };
        if ok {
            Ok(())
        } else {
            Err(Error::DaemonError)
        }
    }
}
