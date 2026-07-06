use std::fmt::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use crate::DinocoValue;

const UNCONFIGURED_SNOWFLAKE_NODE_ID: i64 = -1;
const MAX_SNOWFLAKE_NODE_ID: i64 = 0x3ff;

#[derive(Clone, Debug)]
pub struct DinocoClientConfig {
    pub query_logger: DinocoQueryLogger,
    pub snowflake_node_id: Option<i64>,
}

impl DinocoClientConfig {
    pub fn with_query_logger(mut self, query_logger: DinocoQueryLogger) -> Self {
        self.query_logger = query_logger;
        self
    }

    pub fn with_snowflake_node_id(mut self, snowflake_node_id: i64) -> Self {
        self.snowflake_node_id = Some(sanitize_snowflake_node_id(snowflake_node_id));
        self
    }

    pub(crate) fn initialize_runtime(&self) {
        if let Some(node_id) = self.snowflake_node_id {
            SNOWFLAKE_NODE_ID.store(sanitize_snowflake_node_id(node_id), Ordering::Relaxed);
        }
    }
}

impl Default for DinocoClientConfig {
    fn default() -> Self {
        Self { query_logger: DinocoQueryLogger::disabled(), snowflake_node_id: None }
    }
}

#[derive(Clone)]
pub struct DinocoQueryLog {
    pub adapter: &'static str,
    pub duration: Duration,
    pub params: Vec<DinocoValue>,
    pub query: String,
}

#[derive(Clone, Debug)]
pub struct DinocoQueryLoggerOptions {
    pub include_adapter: bool,
    pub include_duration: bool,
    pub include_params: bool,
    pub include_query: bool,
    pub label: String,
}

impl DinocoQueryLoggerOptions {
    pub fn compact() -> Self {
        Self {
            include_adapter: false,
            include_duration: true,
            include_params: false,
            include_query: true,
            label: "Dinoco Query".to_string(),
        }
    }

    pub fn verbose() -> Self {
        Self {
            include_adapter: true,
            include_duration: true,
            include_params: true,
            include_query: true,
            label: "Dinoco Query".to_string(),
        }
    }
}

impl Default for DinocoQueryLoggerOptions {
    fn default() -> Self {
        Self::verbose()
    }
}

pub trait DinocoQueryLogWriter: Send + Sync {
    fn write(&self, message: &str);
}

#[derive(Clone)]
pub struct DinocoQueryLogger {
    options: DinocoQueryLoggerOptions,
    writer: Option<Arc<dyn DinocoQueryLogWriter>>,
}

impl DinocoQueryLogger {
    pub fn disabled() -> Self {
        Self { options: DinocoQueryLoggerOptions::default(), writer: None }
    }

    pub fn stdout(options: DinocoQueryLoggerOptions) -> Self {
        Self { options, writer: Some(Arc::new(DinocoStdoutQueryLogWriter)) }
    }

    pub fn stderr(options: DinocoQueryLoggerOptions) -> Self {
        Self { options, writer: Some(Arc::new(DinocoStderrQueryLogWriter)) }
    }

    pub fn custom<W>(writer: W, options: DinocoQueryLoggerOptions) -> Self
    where
        W: DinocoQueryLogWriter + 'static,
    {
        Self { options, writer: Some(Arc::new(writer)) }
    }

    pub fn log(&self, log: DinocoQueryLog) {
        let Some(writer) = &self.writer else {
            return;
        };

        writer.write(&self.format(&log));
    }

    pub fn format(&self, log: &DinocoQueryLog) -> String {
        let mut output = String::new();
        let mut sections = Vec::new();

        sections.push(self.options.label.clone());

        if self.options.include_adapter {
            sections.push(format!("adapter={}", log.adapter));
        }

        if self.options.include_duration {
            sections.push(format!("duration={}ms", log.duration.as_secs_f64() * 1000.0));
        }

        if !sections.is_empty() {
            output.push('[');
            output.push_str(&sections.join(" | "));
            output.push(']');
            output.push(' ');
        }

        if self.options.include_query {
            let _ = write!(&mut output, "query={}", log.query);
        }

        if self.options.include_params {
            if self.options.include_query {
                output.push(' ');
            }

            let _ = write!(&mut output, "params={:?}", log.params);
        }

        output
    }
}

impl std::fmt::Debug for DinocoQueryLogger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DinocoQueryLogger")
            .field("options", &self.options)
            .field("enabled", &self.writer.is_some())
            .finish()
    }
}

pub fn current_snowflake_node_id() -> Option<i64> {
    let node_id = SNOWFLAKE_NODE_ID.load(Ordering::Relaxed);

    if node_id == UNCONFIGURED_SNOWFLAKE_NODE_ID { None } else { Some(node_id) }
}

fn sanitize_snowflake_node_id(node_id: i64) -> i64 {
    node_id & MAX_SNOWFLAKE_NODE_ID
}

struct DinocoStdoutQueryLogWriter;

impl DinocoQueryLogWriter for DinocoStdoutQueryLogWriter {
    fn write(&self, message: &str) {
        println!("{message}");
    }
}

struct DinocoStderrQueryLogWriter;

impl DinocoQueryLogWriter for DinocoStderrQueryLogWriter {
    fn write(&self, message: &str) {
        eprintln!("{message}");
    }
}

static SNOWFLAKE_NODE_ID: AtomicI64 = AtomicI64::new(UNCONFIGURED_SNOWFLAKE_NODE_ID);
