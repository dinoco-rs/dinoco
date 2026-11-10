//! Structs kept in a column of another type: `#[derive(DinocoRuntimeStruct)]`
//! and `field.set_runtime_struct(...)` in `dinoco/transform.rs`.

use dinoco_engine::PluckColumn;

use crate::{NumericValue, OrderedValue};

/// A struct stored in a column of another type: a newtype such as
/// `struct Amount(i64)` in an `Integer` column, or any serde type in a `Json`
/// column. Derive it with `#[derive(DinocoRuntimeStruct)]` and switch a model
/// field to it with `field.set_runtime_struct(...)` in `dinoco/transform.rs`.
///
/// The derive reads and writes the column through:
///
/// - a newtype (`struct Amount(i64)`): its field, and `TryFrom<i64>` (or
///   `From<i64>`, which provides it) to read a value back;
/// - `#[dinoco(json)]`: `serde::Serialize` and `serde::Deserialize`, through
///   `serde_json::Value`;
/// - `#[dinoco(column = i64)]` on any other struct: `From<&Self> for i64` to
///   write it and `TryFrom<i64>` to read it.
///
/// ```ignore
/// #[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, DinocoRuntimeStruct)]
/// pub struct Amount(i64);
///
/// impl From<i64> for Amount {
///     fn from(cents: i64) -> Self {
///         Amount(cents)
///     }
/// }
/// ```
pub trait DinocoRuntimeStruct: Sized {
    /// The type the column is read and written as.
    type Column;

    /// The value written to the column.
    fn to_column(&self) -> Self::Column;

    /// The struct read back from a column value, or why it can't be.
    fn from_column(column: Self::Column) -> Result<Self, String>;
}

// A struct over a number sums and averages like the number, and a struct over
// any ordered column has a `min` and a `max`; each is read back as the struct.
impl<T> NumericValue for T
where
    T: DinocoRuntimeStruct + PluckColumn,
    T::Column: NumericValue,
{
    type Sum = T;
    const SUM: dinoco_engine::AggregateFunction = <T::Column as NumericValue>::SUM;
}

impl<T> NumericValue for Option<T>
where
    T: DinocoRuntimeStruct + PluckColumn,
    T::Column: NumericValue,
{
    type Sum = T;
    const SUM: dinoco_engine::AggregateFunction = <T::Column as NumericValue>::SUM;
}

impl<T> OrderedValue for T
where
    T: DinocoRuntimeStruct + PluckColumn,
    T::Column: OrderedValue,
{
    type Value = T;
}

impl<T> OrderedValue for Option<T>
where
    T: DinocoRuntimeStruct + PluckColumn,
    T::Column: OrderedValue,
{
    type Value = T;
}
