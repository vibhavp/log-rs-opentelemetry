use lazy_static::lazy_static;
#[cfg(feature = "kv_unstable")]
use log::kv::{Key, Value, Visitor};
use log::{Level, Log, Metadata};
#[cfg(feature = "kv_unstable")]
use opentelemetry::log::LogError;
use opentelemetry::{
    sdk::log::{Any, LogEmitter, LogEmitterProvider, LogRecord, Severity},
    Context,
};

use opentelemetry_semantic_conventions::trace;
use std::{borrow::Cow, collections::BTreeMap, time::SystemTime};

#[cfg(feature = "kv_unstable")]
use sval::value::Value as SvalValue;

#[cfg(feature = "kv_unstable")]
pub mod any_sval;

/// OpenTelemetry logger. Implements the `Log` trait.
#[derive(Debug)]
pub struct Logger<F: Fn() -> SystemTime + Send + Sync> {
    emitter: LogEmitter,
    resource: Option<BTreeMap<Cow<'static, str>, Any>>,
    timestamp: F,
}

const LOG_EMITTER_NAME: &str = "github.com/vibhavp/log-opentelemetry";

lazy_static! {
    static ref CODE_LINENO: String = trace::CODE_LINENO.to_string();
    static ref CODE_FILEPATH: String = trace::CODE_FILEPATH.to_string();
    static ref CODE_NAMESPACE: String = trace::CODE_NAMESPACE.to_string();
}

impl<F: Fn() -> SystemTime + Send + Sync> Log for Logger<F> {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level() && metadata.level() <= log::STATIC_MAX_LEVEL
    }

    fn log(&self, record: &log::Record) {
        let context = Context::current();
        let mut attrs = BTreeMap::<Cow<'static, str>, Any>::new();

        if let Some(line) = record.line() {
            attrs.insert(Cow::Borrowed(&CODE_LINENO), line.into());
        }

        if let Some(module_path) = record.module_path_static().map_or_else(
            || record.module_path().map(|s| Cow::Owned(s.into())),
            |m| Some(Cow::Borrowed(m)),
        ) {
            attrs.insert(Cow::Borrowed(&CODE_NAMESPACE), module_path.into());
        }

        if let Some(file_path) = record.file_static().map_or_else(
            || record.file().map(|s| Cow::Owned(s.into())),
            |m| Some(Cow::Borrowed(m)),
        ) {
            attrs.insert(Cow::Borrowed(&CODE_FILEPATH), file_path.into());
        }

        let mut record_builder = LogRecord::builder()
            .with_context(&context)
            .with_timestamp((self.timestamp)())
            .with_severity_number(level_to_severity(record.level()))
            .with_severity_text(record.level().as_str());

        record_builder = if let Some(body_str) = record.args().as_str() {
            record_builder.with_body(body_str.into())
        } else {
            record_builder.with_body(record.args().to_string().into())
        };

        if let Some(ref resource) = self.resource {
            record_builder = record_builder.with_resource(resource.clone());
        }

        #[cfg(feature = "kv_unstable")]
        {
            let source = record.key_values();
            let mut visitor = OtelVisitor(BTreeMap::new());
            if let Err(e) = source.visit(&mut visitor) {
                opentelemetry::global::handle_error::<LogError>(LogError::Other(e.into()));
            }
        }

        self.emitter.emit(record_builder.build());
    }

    fn flush(&self) {
        if let Some(provider) = self.emitter.provider() {
            provider.force_flush();
        }
    }
}

/// A builder for [`Logger`]
#[derive(Debug)]
pub struct Builder<F: Fn() -> SystemTime + Send + Sync> {
    resource: Option<BTreeMap<Cow<'static, str>, Any>>,
    timestamp: F,
}

impl<F> Builder<F>
where
    F: Fn() -> SystemTime + Send + Sync,
{
    pub fn new(f: F) -> Self {
        Self {
            timestamp: f,
            resource: None,
        }
    }

    /// Set resource for all emitted Records.
    pub fn with_resource(self, resource: BTreeMap<Cow<'static, str>, Any>) -> Self {
        Self {
            resource: Some(resource),
            ..self
        }
    }

    /// Build a `Logger`, consuming this builder.
    pub fn build(self, emitter_provider: &LogEmitterProvider) -> Logger<F> {
        Logger {
            emitter: emitter_provider
                .versioned_log_emitter(LOG_EMITTER_NAME, Some(env!("CARGO_PKG_VERSION"))),
            resource: self.resource,
            timestamp: self.timestamp,
        }
    }
}

#[cfg(feature = "kv_unstable")]
struct OtelVisitor(BTreeMap<Cow<'static, str>, Any>);

#[cfg(feature = "kv_unstable")]
impl<'kvs> Visitor<'kvs> for OtelVisitor {
    fn visit_pair(&mut self, key: Key<'kvs>, value: Value<'kvs>) -> Result<(), log::kv::Error> {
        let mut stream = any_sval::SvalAny::stream();
        value
            .stream(&mut stream.borrow_mut())
            .map_err(|e| e.into_io_error())?;
        if let Some(v) = stream.into_inner().value() {
            self.0.insert(key.as_str().to_string().into(), v);
        }
        Ok(())
    }
}

fn level_to_severity(level: Level) -> Severity {
    match level {
        Level::Error => Severity::Error,
        Level::Warn => Severity::Warn,
        Level::Info => Severity::Info,
        Level::Debug => Severity::Debug,
        Level::Trace => Severity::Trace,
    }
}
