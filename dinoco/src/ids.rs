use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Uuid;

const DINOCO_SNOWFLAKE_EPOCH: i64 = 1_700_000_000_000;
const MAX_NODE_ID: i64 = 0x3ff;
const MAX_SEQUENCE: i64 = 0xfff;

struct SnowflakeState {
    last_timestamp: i64,
    node_id: i64,
    sequence: i64,
}

pub fn uuid_v7() -> Uuid {
    Uuid::now_v7()
}

pub fn snowflake() -> i64 {
    let state = SNOWFLAKE_STATE
        .get_or_init(|| Mutex::new(SnowflakeState { last_timestamp: -1, node_id: load_node_id(), sequence: 0 }));
    let mut state = state.lock().expect("failed to lock Dinoco snowflake state");
    let mut timestamp = current_timestamp();

    if timestamp == state.last_timestamp {
        state.sequence = (state.sequence + 1) & MAX_SEQUENCE;

        if state.sequence == 0 {
            timestamp = wait_next_timestamp(state.last_timestamp);
        }
    } else {
        state.sequence = 0;
    }

    state.last_timestamp = timestamp;

    ((timestamp - DINOCO_SNOWFLAKE_EPOCH) << 22) | ((state.node_id & MAX_NODE_ID) << 12) | state.sequence
}

fn current_timestamp() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock is before UNIX_EPOCH").as_millis() as i64
}

fn load_node_id() -> i64 {
    let value = std::env::var("NODE_ID").expect("missing NODE_ID environment variable required by dinoco::snowflake()");
    let node_id = value.parse::<i64>().expect("invalid NODE_ID environment variable required by dinoco::snowflake()");

    node_id & MAX_NODE_ID
}

fn wait_next_timestamp(last_timestamp: i64) -> i64 {
    let mut timestamp = current_timestamp();

    while timestamp <= last_timestamp {
        timestamp = current_timestamp();
    }

    timestamp
}

static SNOWFLAKE_STATE: OnceLock<Mutex<SnowflakeState>> = OnceLock::new();
