use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use dinoco_engine::{
    DinocoClientConfig, DinocoQueryLog, DinocoQueryLogWriter, DinocoQueryLogger, DinocoQueryLoggerOptions,
    DinocoRedisConfig, DinocoValue, SqliteAdapter, current_snowflake_node_id,
};

#[derive(Clone)]
struct MemoryWriter {
    logs: Arc<Mutex<Vec<String>>>,
}

impl MemoryWriter {
    fn new() -> Self {
        Self { logs: Arc::new(Mutex::new(Vec::new())) }
    }

    fn entries(&self) -> Vec<String> {
        self.logs.lock().expect("memory logger should lock").clone()
    }
}

impl DinocoQueryLogWriter for MemoryWriter {
    fn write(&self, message: &str) {
        self.logs.lock().expect("memory logger should lock").push(message.to_string());
    }
}

#[test]
fn verbose_logger_formats_query_params_and_duration() {
    let logger = DinocoQueryLogger::custom(MemoryWriter::new(), DinocoQueryLoggerOptions::verbose());
    let message = logger.format(&DinocoQueryLog {
        adapter: "sqlite",
        duration: Duration::from_millis(12),
        params: vec![DinocoValue::Integer(7), DinocoValue::String("dinoco".to_string())],
        query: "SELECT * FROM users WHERE id = ?".to_string(),
    });

    assert!(message.contains("adapter=sqlite"));
    assert!(message.contains("duration=12"));
    assert!(message.contains("query=SELECT * FROM users WHERE id = ?"));
    assert!(message.contains("params=[Integer(7), String(\"dinoco\")]"));
}

#[test]
fn compact_logger_can_skip_params() {
    let logger = DinocoQueryLogger::custom(MemoryWriter::new(), DinocoQueryLoggerOptions::compact());
    let message = logger.format(&DinocoQueryLog {
        adapter: "postgres",
        duration: Duration::from_millis(3),
        params: vec![DinocoValue::Integer(99)],
        query: "DELETE FROM users WHERE id = $1".to_string(),
    });

    assert!(!message.contains("adapter="));
    assert!(!message.contains("params="));
    assert!(message.contains("duration=3"));
    assert!(message.contains("DELETE FROM users"));
}

#[test]
fn redis_config_builds_connection_url_from_parameters() {
    let redis = DinocoRedisConfig::from_host("localhost:6379").with_username("dinoco").with_password("secret");

    assert_eq!(redis.connection_url(), "redis://dinoco:secret@localhost:6379");
}

#[test]
fn client_config_can_store_redis_configuration() {
    let config = DinocoClientConfig::default().with_redis(DinocoRedisConfig::from_url("redis://127.0.0.1:6379"));

    assert!(config.redis.is_some());
    assert!(matches!(config.redis, Some(DinocoRedisConfig::Url { .. })));
}

#[test]
fn custom_logger_receives_messages() {
    let writer = MemoryWriter::new();
    let logger = DinocoQueryLogger::custom(writer.clone(), DinocoQueryLoggerOptions::verbose());

    logger.log(DinocoQueryLog {
        adapter: "mysql",
        duration: Duration::from_millis(8),
        params: vec![DinocoValue::Boolean(true)],
        query: "UPDATE users SET active = ?".to_string(),
    });

    assert_eq!(writer.entries().len(), 1);
    assert!(writer.entries()[0].contains("UPDATE users SET active = ?"));
}

#[tokio::test]
async fn config_initializes_snowflake_node_id() {
    let _client = dinoco_engine::DinocoClient::<SqliteAdapter>::new(
        "file::memory:".to_string(),
        vec![],
        DinocoClientConfig::default().with_snowflake_node_id(2049),
    )
    .await
    .expect("sqlite client should initialize runtime");

    assert_eq!(current_snowflake_node_id(), Some(1));
}
