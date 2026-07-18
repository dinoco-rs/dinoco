use std::marker::PhantomData;

use dinoco_engine::{DeleteQuery, DinocoClient, DinocoValue, FindWhere, InsertQuery, UpdateOperation, UpdateSet};

pub struct UpdateField<T> {
    name: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> UpdateField<T> {
    pub const fn new(name: &'static str) -> Self {
        Self { name, marker: PhantomData }
    }

    pub fn set<V>(self, value: V) -> UpdateSet
    where
        V: IntoUpdateValue<T>,
    {
        UpdateSet { field: self.name, value: value.into_update_value(), operation: UpdateOperation::Set }
    }

    pub fn connect<V>(self, value: V) -> UpdateSet
    where
        V: IntoUpdateValue<T>,
    {
        UpdateSet { field: self.name, value: value.into_update_value(), operation: UpdateOperation::Connect }
    }

    pub fn disconnect<V>(self, value: V) -> UpdateSet
    where
        V: IntoUpdateValue<T>,
    {
        UpdateSet { field: self.name, value: value.into_update_value(), operation: UpdateOperation::Disconnect }
    }
}

pub trait IntoUpdateValue<T> {
    fn into_update_value(self) -> DinocoValue;
}

impl IntoUpdateValue<String> for String {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl IntoUpdateValue<String> for &String {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl IntoUpdateValue<String> for &str {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl IntoUpdateValue<bool> for bool {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl IntoUpdateValue<bool> for &bool {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

macro_rules! impl_update_integer {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoUpdateValue<$ty> for $ty {
                fn into_update_value(self) -> DinocoValue {
                    self.into()
                }
            }

            impl IntoUpdateValue<$ty> for &$ty {
                fn into_update_value(self) -> DinocoValue {
                    self.into()
                }
            }
        )*
    };
}

impl_update_integer!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

impl IntoUpdateValue<f64> for f64 {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl IntoUpdateValue<f32> for f32 {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl IntoUpdateValue<f32> for &f32 {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl IntoUpdateValue<f64> for &f64 {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl IntoUpdateValue<Vec<u8>> for Vec<u8> {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl IntoUpdateValue<Vec<u8>> for &Vec<u8> {
    fn into_update_value(self) -> DinocoValue {
        self.into()
    }
}

impl<T> IntoUpdateValue<Option<T>> for Option<T>
where
    for<'a> &'a T: Into<DinocoValue>,
{
    fn into_update_value(self) -> DinocoValue {
        match self {
            Some(value) => (&value).into(),
            None => DinocoValue::Null,
        }
    }
}

pub(crate) fn split_update_sets(sets: Vec<UpdateSet>) -> (Vec<UpdateSet>, Vec<UpdateSet>, Vec<UpdateSet>) {
    let mut updates = Vec::new();
    let mut connects = Vec::new();
    let mut disconnects = Vec::new();

    for set in sets {
        match set.operation {
            UpdateOperation::Set => updates.push(set),
            UpdateOperation::Connect => connects.push(set),
            UpdateOperation::Disconnect => disconnects.push(set),
        }
    }

    (updates, connects, disconnects)
}

pub(crate) async fn execute_relation_update_sets(
    table: &'static str,
    conditions: &[FindWhere],
    connects: Vec<UpdateSet>,
    disconnects: Vec<UpdateSet>,
    client: &DinocoClient,
) -> anyhow::Result<()> {
    for connect in connects {
        let rows = connect_rows(conditions, connect.field, connect.value)?;

        if !rows.is_empty() {
            let fields = connect_fields(conditions, connect.field)?;
            let query = InsertQuery { table, fields, rows, returning: None };
            client.backend.insert(query).await?;
        }
    }

    for disconnect in disconnects {
        for condition_group in disconnect_conditions(conditions, disconnect.field, disconnect.value)? {
            let query = DeleteQuery { table, conditions: condition_group, returning: None };
            client.backend.delete(query).await?;
        }
    }

    Ok(())
}

fn connect_fields(conditions: &[FindWhere], connect_field: &'static str) -> anyhow::Result<Vec<&'static str>> {
    let mut fields = connect_source_fields(conditions)?;
    fields.push(connect_field);

    Ok(fields)
}

fn connect_rows(
    conditions: &[FindWhere],
    connect_field: &'static str,
    connect_value: DinocoValue,
) -> anyhow::Result<Vec<Vec<DinocoValue>>> {
    let sources = connect_source_values(conditions)?;
    let mut rows = Vec::with_capacity(sources.len());

    for mut source_values in sources {
        source_values.push(connect_value.clone());
        rows.push(source_values);
    }

    if rows.is_empty() {
        anyhow::bail!(
            "connect on '{}' requires at least one .where_(|x| x.some_id.eq(...)) or .batch(...).",
            connect_field
        );
    }

    Ok(rows)
}

fn disconnect_conditions(
    conditions: &[FindWhere],
    disconnect_field: &'static str,
    disconnect_value: DinocoValue,
) -> anyhow::Result<Vec<Vec<FindWhere>>> {
    let sources = expand_conditions(conditions)?;

    if sources.is_empty() {
        anyhow::bail!(
            "disconnect on '{}' requires at least one .where_(|x| x.some_id.eq(...)) or .batch(...).",
            disconnect_field
        );
    }

    Ok(sources
        .into_iter()
        .map(|mut condition_group| {
            condition_group.push(FindWhere::Eq(disconnect_field, disconnect_value.clone()));
            condition_group
        })
        .collect())
}

fn connect_source_fields(conditions: &[FindWhere]) -> anyhow::Result<Vec<&'static str>> {
    let mut fields = Vec::new();

    for condition in conditions {
        match condition {
            FindWhere::Eq(field, _) | FindWhere::Batch(field, _) => fields.push(*field),
            _ => anyhow::bail!("connect/disconnect only supports eq(...) and batch(...) filters for pivot keys."),
        }
    }

    Ok(fields)
}

fn connect_source_values(conditions: &[FindWhere]) -> anyhow::Result<Vec<Vec<DinocoValue>>> {
    expand_conditions(conditions)?
        .into_iter()
        .map(|condition_group| {
            condition_group
                .into_iter()
                .map(|condition| match condition {
                    FindWhere::Eq(_, value) => Ok(value),
                    _ => anyhow::bail!("connect only supports eq(...) and batch(...) filters for pivot keys."),
                })
                .collect()
        })
        .collect()
}

fn expand_conditions(conditions: &[FindWhere]) -> anyhow::Result<Vec<Vec<FindWhere>>> {
    let mut groups = vec![Vec::new()];

    for condition in conditions {
        match condition {
            FindWhere::Eq(field, value) => {
                for group in &mut groups {
                    group.push(FindWhere::Eq(field, value.clone()));
                }
            }
            FindWhere::Batch(field, values) => {
                let mut next = Vec::new();

                for group in &groups {
                    for value in values {
                        let mut next_group = group.clone();
                        next_group.push(FindWhere::Eq(field, value.clone()));
                        next.push(next_group);
                    }
                }

                groups = next;
            }
            _ => anyhow::bail!("connect/disconnect only supports eq(...) and batch(...) filters for pivot keys."),
        }
    }

    Ok(groups)
}
