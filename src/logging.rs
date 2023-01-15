use std::sync::Arc;

use anyhow::Result;
use spdlog::{
    sink::{
        AsyncPoolSink, FileSink, RotatingFileSink, RotationPolicy, Sink,
        StdStream, StdStreamSink,
    },
    Level, LevelFilter, Logger, LoggerBuilder,
};

pub fn builder(
    file: impl Into<std::path::PathBuf>,
    append_date: bool,
) -> Result<LoggerBuilder> {
    let mut sinks: Vec<Arc<dyn Sink>> = vec![
        Arc::new(
            StdStreamSink::builder()
                .std_stream(StdStream::Stdout)
                .level_filter(LevelFilter::MoreVerbose(Level::Warn))
                .build()?,
        ),
        Arc::new(
            StdStreamSink::builder()
                .std_stream(StdStream::Stderr)
                .level_filter(LevelFilter::MoreSevereEqual(Level::Warn))
                .build()?,
        ),
    ];

    if append_date {
        sinks.push(Arc::new(
            RotatingFileSink::builder()
                .base_path(file)
                .rotation_policy(RotationPolicy::Daily { hour: 0, minute: 0 })
                .rotate_on_open(true)
                .build()?,
        ));
    } else {
        sinks.push(Arc::new(
            FileSink::builder().path(file).truncate(false).build()?,
        ));
    };

    let async_sink = Arc::new(AsyncPoolSink::builder().sinks(sinks).build()?);

    let mut builder = Logger::builder();
    builder.flush_level_filter(LevelFilter::MoreSevereEqual(Level::Warn));
    builder.sink(async_sink);

    Ok(builder)
}
