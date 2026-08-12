use std::marker::PhantomData;

use dinoco_engine::{DinocoValue, FindWhere};

/// Capability generated only for fields declared with `@fulltext`.
///
/// Regular string fields intentionally do not expose full-text search.
pub struct FullTextField;

pub struct Field<T, Capability = ()> {
    name: &'static str,
    fulltext_fields: &'static [&'static str],
    marker: PhantomData<fn() -> (T, Capability)>,
}

impl<T, Capability> Clone for Field<T, Capability> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, Capability> Copy for Field<T, Capability> {}

impl<T, Capability> Field<T, Capability> {
    pub const fn new(name: &'static str) -> Self {
        Self { name, fulltext_fields: &[], marker: PhantomData }
    }

    #[doc(hidden)]
    pub const fn new_fulltext(name: &'static str, fulltext_fields: &'static [&'static str]) -> Self {
        Self { name, fulltext_fields, marker: PhantomData }
    }

    pub fn eq<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Eq(self.name, value.into())
    }

    pub fn neq<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Neq(self.name, value.into())
    }

    pub fn gt<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Gt(self.name, value.into())
    }

    pub fn gte<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Gte(self.name, value.into())
    }

    pub fn lt<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Lt(self.name, value.into())
    }

    pub fn lte<V>(self, value: V) -> FindWhere
    where
        V: Into<DinocoValue>,
    {
        FindWhere::Lte(self.name, value.into())
    }

    pub fn batch<I, V>(self, values: I) -> FindWhere
    where
        I: IntoIterator<Item = V>,
        V: Into<DinocoValue>,
    {
        FindWhere::Batch(self.name, values.into_iter().map(Into::into).collect())
    }

    pub fn null(self) -> FindWhere {
        FindWhere::Null(self.name)
    }

    pub fn not_null(self) -> FindWhere {
        FindWhere::NotNull(self.name)
    }
}

macro_rules! impl_string_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<Capability> Field<$ty, Capability> {
                pub fn like<V>(self, value: V) -> FindWhere
                where
                    V: AsRef<str>,
                {
                    FindWhere::Like(self.name, DinocoValue::String(format!("%{}%", value.as_ref())))
                }

                pub fn starts_with<V>(self, value: V) -> FindWhere
                where
                    V: AsRef<str>,
                {
                    FindWhere::Like(self.name, DinocoValue::String(format!("{}%", value.as_ref())))
                }

                pub fn ends_with<V>(self, value: V) -> FindWhere
                where
                    V: AsRef<str>,
                {
                    FindWhere::Like(self.name, DinocoValue::String(format!("%{}", value.as_ref())))
                }
            }
        )*
    };
}

macro_rules! impl_between_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl Field<$ty> {
                pub fn between<V>(self, start: V, end: V) -> FindWhere
                where
                    V: Into<DinocoValue>,
                {
                    FindWhere::Between(self.name, start.into(), end.into())
                }
            }
        )*
    };
}

impl_string_field!(String, Option<String>);

impl Field<String, FullTextField> {
    pub fn fulltext<V>(self, value: V) -> FindWhere
    where
        V: AsRef<str>,
    {
        FindWhere::FullText(self.fulltext_fields, DinocoValue::String(value.as_ref().to_string()))
    }
}

impl Field<Option<String>, FullTextField> {
    pub fn fulltext<V>(self, value: V) -> FindWhere
    where
        V: AsRef<str>,
    {
        FindWhere::FullText(self.fulltext_fields, DinocoValue::String(value.as_ref().to_string()))
    }
}
impl_between_field!(
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    f32,
    f64,
    Option<i8>,
    Option<i16>,
    Option<i32>,
    Option<i64>,
    Option<i128>,
    Option<isize>,
    Option<u8>,
    Option<u16>,
    Option<u32>,
    Option<u64>,
    Option<u128>,
    Option<usize>,
    Option<f32>,
    Option<f64>,
    dinoco_engine::chrono::DateTime<dinoco_engine::chrono::Utc>,
    dinoco_engine::chrono::NaiveDate,
    Option<dinoco_engine::chrono::DateTime<dinoco_engine::chrono::Utc>>,
    Option<dinoco_engine::chrono::NaiveDate>,
);
