use std::marker::PhantomData;

use dinoco_engine::{DinocoValue, FindWhere};

pub struct Field<T> {
    name: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> Field<T> {
    pub const fn new(name: &'static str) -> Self {
        Self { name, marker: PhantomData }
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
