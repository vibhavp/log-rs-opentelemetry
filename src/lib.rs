use log::{Level, Log, Metadata};
use opentelemetry::{
    logs::{LogRecordBuilder, Logger, LoggerProvider, Severity},
    KeyValue,
};

use opentelemetry_semantic_conventions::trace;
use std::borrow::Cow;

const LOGGER_NAME: &str = "github.com/vibhavp/log-opentelemetry";
const LOGGER_VERSION: &str = "0.0.1";

pub struct LogBridge<L> {
    logger: L,
}

impl<L> LogBridge<L> {
    pub fn new<P>(provider: P) -> Self
    where
        P: LoggerProvider<Logger = L>,
    {
        LogBridge {
            logger: provider.logger(LOGGER_NAME),
        }
    }

    pub fn new_versioned<P>(
        provider: P,
        schema_url: Option<Cow<'static, str>>,
        attributes: Option<Vec<KeyValue>>,
        include_trace_context: bool,
    ) -> Self
    where
        P: LoggerProvider<Logger = L>,
    {
        LogBridge {
            logger: provider.versioned_logger(
                LOGGER_NAME,
                Some(Cow::Borrowed(LOGGER_VERSION)),
                schema_url,
                attributes,
                include_trace_context,
            ),
        }
    }
}

impl<L: Logger + Send + Sync> Log for LogBridge<L> {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level() && metadata.level() <= log::STATIC_MAX_LEVEL
    }

    fn log(&self, record: &log::Record) {
        let mut builder = LogRecordBuilder::new()
            .with_severity_number(level_to_severity(record.level()))
            .with_severity_text(record.level().as_str());

        if let Some(line) = record.line() {
            builder = builder.with_attribute(trace::CODE_LINENO, line);
        }

        if let Some(module_path) = record.module_path_static().map_or_else(
            || record.module_path().map(|s| Cow::Owned(s.into())),
            |m| Some(Cow::Borrowed(m)),
        ) {
            builder = builder.with_attribute(trace::CODE_NAMESPACE, module_path);
        }

        if let Some(file_path) = record.file_static().map_or_else(
            || record.file().map(|s| Cow::Owned(s.into())),
            |m| Some(Cow::Borrowed(m)),
        ) {
            builder = builder.with_attribute(trace::CODE_FILEPATH, file_path);
        }

        builder = if let Some(body_str) = record.args().as_str() {
            builder.with_body(body_str.into())
        } else {
            builder.with_body(record.args().to_string().into())
        };

        self.logger.emit(builder.build());
    }

    fn flush(&self) {}
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
