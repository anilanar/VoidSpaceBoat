use std::sync::Arc;

use spdlog::{
    sink::{
        AsyncPoolSink, FileSink, RotatingFileSink, RotationPolicy, Sink,
        StdStream, StdStreamSink,
    },
    Level, LevelFilter, Logger, LoggerBuilder, ThreadPool, ThreadPoolBuilder,
};

use crate::error::ServerError;

pub fn builder(
    file: impl Into<std::path::PathBuf>,
    append_date: bool,
) -> Result<LoggerBuilder, ServerError> {
    let mut sinks: Vec<Arc<dyn Sink>> = vec![
        Arc::new(
            StdStreamSink::builder()
                .std_stream(StdStream::Stdout)
                .level_filter(LevelFilter::MoreVerbose(Level::Warn))
                .build()
                .map_err(ServerError::LoggerError)?,
        ),
        Arc::new(
            StdStreamSink::builder()
                .std_stream(StdStream::Stderr)
                .level_filter(LevelFilter::MoreSevereEqual(Level::Warn))
                .build()
                .map_err(ServerError::LoggerError)?,
        ),
    ];

    if append_date {
        sinks.push(Arc::new(
            RotatingFileSink::builder()
                .base_path(file)
                .rotation_policy(RotationPolicy::Daily { hour: 0, minute: 0 })
                .rotate_on_open(true)
                .build()
                .map_err(ServerError::LoggerError)?,
        ));
    } else {
        sinks.push(Arc::new(
            FileSink::builder()
                .path(file)
                .truncate(false)
                .build()
                .map_err(ServerError::LoggerError)?,
        ));
    };

    let async_sink = Arc::new(
        AsyncPoolSink::builder()
            .sinks(sinks)
            .build()
            .map_err(ServerError::LoggerError)?,
    );

    let mut builder = Logger::builder();
    builder.flush_level_filter(LevelFilter::MoreSevereEqual(Level::Warn));
    builder.sink(async_sink);

    Ok(builder)
}
