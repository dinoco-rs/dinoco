use dinoco_engine::{DinocoRow, DinocoValue};

use crate::IncludeNode;

pub type IncludeApplier<'a, T> = Box<dyn FnOnce(&mut [T]) + 'a>;
pub type IncludeLoaderFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = dinoco_engine::DinocoResult<IncludeApplier<'a, T>>> + 'a>>;

pub trait Model: Sized {
    type Include: Default;
    type Where: Default;

    fn table_name() -> &'static str;
}

pub trait Projection<M: Model>: DinocoRow {
    fn columns() -> &'static [&'static str];

    fn load_includes<'a, A>(
        _items: &'a mut [Self],
        _includes: &'a [IncludeNode],
        _client: &'a dinoco_engine::DinocoClient<A>,
        _read_mode: crate::ReadMode,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = dinoco_engine::DinocoResult<()>> + 'a>>
    where
        Self: Sized,
        A: dinoco_engine::DinocoAdapter,
    {
        Box::pin(async { Ok(()) })
    }
}

pub trait IntoDinocoValue {
    fn into_dinoco_value(self) -> DinocoValue;
}

pub trait IntoIncludeNode {
    fn into_include_node(self) -> IncludeNode;
}

impl IntoDinocoValue for DinocoValue {
    fn into_dinoco_value(self) -> DinocoValue {
        self
    }
}

impl IntoDinocoValue for String {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::from(self)
    }
}

impl IntoDinocoValue for &str {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::from(self)
    }
}

impl IntoDinocoValue for bool {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::from(self)
    }
}

impl IntoDinocoValue for i64 {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::from(self)
    }
}

impl IntoDinocoValue for i32 {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::Integer(self as i64)
    }
}

impl IntoDinocoValue for usize {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::Integer(self as i64)
    }
}

impl IntoDinocoValue for f64 {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::from(self)
    }
}

impl IntoDinocoValue for serde_json::Value {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::Json(self)
    }
}

impl IntoDinocoValue for chrono::DateTime<chrono::Utc> {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::DateTime(self)
    }
}

impl IntoDinocoValue for chrono::NaiveDate {
    fn into_dinoco_value(self) -> DinocoValue {
        DinocoValue::Date(self)
    }
}
