use std::fmt;
use std::ops::Deref;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use dinoco_engine::current_snowflake_node_id;
use serde::{Deserialize, Serialize};

use crate::DinocoValue;

const DINOCO_SNOWFLAKE_EPOCH: i64 = 1_700_000_000_000;
const MAX_NODE_ID: i64 = 0x3ff;
const MAX_SEQUENCE: i64 = 0xfff;

struct SnowflakeState {
    last_timestamp: i64,
    node_id: i64,
    sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Uuid(String);

impl Uuid {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7().to_string())
    }

    pub fn now_v7() -> Self {
        Self::new()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Deref for Uuid {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for Uuid {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<String> for Uuid {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Uuid {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<uuid::Uuid> for Uuid {
    fn from(value: uuid::Uuid) -> Self {
        Self(value.to_string())
    }
}

impl From<Uuid> for String {
    fn from(value: Uuid) -> Self {
        value.0
    }
}

impl TryFrom<DinocoValue> for Uuid {
    type Error = crate::DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        Ok(Self(String::try_from(value)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Snowflake(i64);

impl Snowflake {
    pub fn new() -> Self {
        Self(snowflake())
    }

    pub fn as_i64(&self) -> i64 {
        self.0
    }

    pub fn into_inner(self) -> i64 {
        self.0
    }
}

impl fmt::Display for Snowflake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i64> for Snowflake {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<Snowflake> for i64 {
    fn from(value: Snowflake) -> Self {
        value.0
    }
}

impl TryFrom<DinocoValue> for Snowflake {
    type Error = crate::DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        Ok(Self(i64::try_from(value)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct AutoIncrement(i64);

impl AutoIncrement {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn as_i64(&self) -> i64 {
        self.0
    }

    pub fn into_inner(self) -> i64 {
        self.0
    }
}

impl fmt::Display for AutoIncrement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i64> for AutoIncrement {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<AutoIncrement> for i64 {
    fn from(value: AutoIncrement) -> Self {
        value.0
    }
}

impl TryFrom<DinocoValue> for AutoIncrement {
    type Error = crate::DinocoError;

    fn try_from(value: DinocoValue) -> Result<Self, Self::Error> {
        Ok(Self(i64::try_from(value)?))
    }
}

pub fn uuid_v7() -> uuid::Uuid {
    uuid::Uuid::now_v7()
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
    current_snowflake_node_id()
        .expect("missing snowflake_node_id in DinocoClientConfig required by dinoco::snowflake()")
        & MAX_NODE_ID
}

fn wait_next_timestamp(last_timestamp: i64) -> i64 {
    let mut timestamp = current_timestamp();

    while timestamp <= last_timestamp {
        timestamp = current_timestamp();
    }

    timestamp
}

static SNOWFLAKE_STATE: OnceLock<Mutex<SnowflakeState>> = OnceLock::new();
