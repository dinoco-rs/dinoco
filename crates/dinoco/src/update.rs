use std::collections::HashSet;
use std::marker::PhantomData;

use dinoco_engine::{
    DeleteQuery, DinocoEntity, DinocoProjection, DinocoRowModel, DinocoValue, FindQuery, FindWhere, InsertQuery,
    ManyToManyUpdate, UpdateOperation, UpdateSet,
};

use crate::{DinocoRelationValue, MutationExecutor};

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

macro_rules! impl_numeric_update_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl UpdateField<$ty> {
                pub fn increment<V>(self, value: V) -> UpdateSet
                where
                    V: IntoUpdateValue<$ty>,
                {
                    UpdateSet {
                        field: self.name,
                        value: value.into_update_value(),
                        operation: UpdateOperation::Increment,
                    }
                }

                pub fn decrement<V>(self, value: V) -> UpdateSet
                where
                    V: IntoUpdateValue<$ty>,
                {
                    UpdateSet {
                        field: self.name,
                        value: value.into_update_value(),
                        operation: UpdateOperation::Decrement,
                    }
                }

                pub fn multiply<V>(self, value: V) -> UpdateSet
                where
                    V: IntoUpdateValue<$ty>,
                {
                    UpdateSet {
                        field: self.name,
                        value: value.into_update_value(),
                        operation: UpdateOperation::Multiply,
                    }
                }

                pub fn divide<V>(self, value: V) -> UpdateSet
                where
                    V: IntoUpdateValue<$ty>,
                {
                    UpdateSet {
                        field: self.name,
                        value: value.into_update_value(),
                        operation: UpdateOperation::Divide,
                    }
                }
            }

            impl UpdateField<Option<$ty>> {
                pub fn increment<V>(self, value: V) -> UpdateSet
                where
                    V: IntoUpdateValue<$ty>,
                {
                    UpdateField::<$ty>::new(self.name).increment(value)
                }

                pub fn decrement<V>(self, value: V) -> UpdateSet
                where
                    V: IntoUpdateValue<$ty>,
                {
                    UpdateField::<$ty>::new(self.name).decrement(value)
                }

                pub fn multiply<V>(self, value: V) -> UpdateSet
                where
                    V: IntoUpdateValue<$ty>,
                {
                    UpdateField::<$ty>::new(self.name).multiply(value)
                }

                pub fn divide<V>(self, value: V) -> UpdateSet
                where
                    V: IntoUpdateValue<$ty>,
                {
                    UpdateField::<$ty>::new(self.name).divide(value)
                }
            }
        )*
    };
}

impl_numeric_update_field!(i64, f64);

pub struct ManyToManyUpdateField<T> {
    name: &'static str,
    relation: ManyToManyUpdate,
    marker: PhantomData<fn() -> T>,
}

impl<T> ManyToManyUpdateField<T> {
    pub const fn new(
        name: &'static str,
        join_table: &'static str,
        parent_field: &'static str,
        join_parent_field: &'static str,
        join_child_field: &'static str,
    ) -> Self {
        Self {
            name,
            relation: ManyToManyUpdate { join_table, parent_field, join_parent_field, join_child_field },
            marker: PhantomData,
        }
    }

    pub fn connect<V>(self, value: V) -> UpdateSet
    where
        V: IntoUpdateValue<T>,
    {
        UpdateSet {
            field: self.name,
            value: value.into_update_value(),
            operation: UpdateOperation::ConnectManyToMany(self.relation),
        }
    }

    pub fn disconnect<V>(self, value: V) -> UpdateSet
    where
        V: IntoUpdateValue<T>,
    {
        UpdateSet {
            field: self.name,
            value: value.into_update_value(),
            operation: UpdateOperation::DisconnectManyToMany(self.relation),
        }
    }

    /// Connects every value in `values` in one round trip.
    ///
    /// Equivalent to calling `.connect(...)` once per value, except every
    /// connected pair lands in a single multi-row `INSERT` instead of one
    /// query per value.
    pub fn connect_batch<I, V>(self, values: I) -> Vec<UpdateSet>
    where
        I: IntoIterator<Item = V>,
        V: IntoUpdateValue<T>,
    {
        values
            .into_iter()
            .map(|value| UpdateSet {
                field: self.name,
                value: value.into_update_value(),
                operation: UpdateOperation::ConnectManyToMany(self.relation),
            })
            .collect()
    }

    /// Disconnects every value in `values` in one round trip.
    ///
    /// Equivalent to calling `.disconnect(...)` once per value, except every
    /// disconnected side collapses into a single `DELETE ... WHERE child IN
    /// (...)` instead of one query per value.
    pub fn disconnect_batch<I, V>(self, values: I) -> Vec<UpdateSet>
    where
        I: IntoIterator<Item = V>,
        V: IntoUpdateValue<T>,
    {
        values
            .into_iter()
            .map(|value| UpdateSet {
                field: self.name,
                value: value.into_update_value(),
                operation: UpdateOperation::DisconnectManyToMany(self.relation),
            })
            .collect()
    }
}

/// Lets `.update(...)` accept either a single [`UpdateSet`] (from `.set(...)`,
/// `.connect(...)`, ...) or a `Vec<UpdateSet>` (from `.connect_batch(...)`,
/// `.disconnect_batch(...)`), so both shapes can be pushed onto the same
/// builder without a separate method.
pub trait IntoUpdateSets {
    fn into_update_sets(self) -> Vec<UpdateSet>;
}

impl IntoUpdateSets for UpdateSet {
    fn into_update_sets(self) -> Vec<UpdateSet> {
        vec![self]
    }
}

impl IntoUpdateSets for Vec<UpdateSet> {
    fn into_update_sets(self) -> Vec<UpdateSet> {
        self
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

macro_rules! impl_update_value {
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

impl_update_value!(
    dinoco_engine::serde_json::Value,
    dinoco_engine::chrono::DateTime<dinoco_engine::chrono::Utc>,
    dinoco_engine::chrono::NaiveDate,
);

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

impl<T> IntoUpdateValue<Option<T>> for &Option<T>
where
    for<'a> &'a T: Into<DinocoValue>,
{
    fn into_update_value(self) -> DinocoValue {
        match self {
            Some(value) => value.into(),
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
            UpdateOperation::Set
            | UpdateOperation::Increment
            | UpdateOperation::Decrement
            | UpdateOperation::Multiply
            | UpdateOperation::Divide => updates.push(set),
            UpdateOperation::Connect | UpdateOperation::ConnectManyToMany(_) => connects.push(set),
            UpdateOperation::Disconnect | UpdateOperation::DisconnectManyToMany(_) => disconnects.push(set),
        }
    }

    (updates, connects, disconnects)
}

pub(crate) fn duplicate_update_field(sets: &[UpdateSet]) -> Option<&'static str> {
    let mut fields = HashSet::with_capacity(sets.len());

    // Many-to-many connect/disconnect operations are expected to repeat the
    // same field: that's how multiple values get connected, whether through
    // several `.connect(...)` calls or one `.connect_batch(...)`.
    sets.iter()
        .filter(|set| {
            !matches!(set.operation, UpdateOperation::ConnectManyToMany(_) | UpdateOperation::DisconnectManyToMany(_))
        })
        .find_map(|set| (!fields.insert(set.field)).then_some(set.field))
}

pub(crate) fn has_many_to_many_update_sets(sets: &[UpdateSet]) -> bool {
    sets.iter().any(|set| {
        matches!(set.operation, UpdateOperation::ConnectManyToMany(_) | UpdateOperation::DisconnectManyToMany(_))
    })
}

pub(crate) async fn load_update_matches<M, S, C>(conditions: &[FindWhere], client: &C) -> anyhow::Result<Vec<S>>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
    C: MutationExecutor,
{
    let mut query = FindQuery::new(S::FIELDS, M::TABLE_NAME, -1, -1);
    query.conditions = conditions.to_vec();
    client.query::<S>(query).await
}

pub(crate) async fn execute_relation_update_sets<M, C>(
    table: &'static str,
    conditions: &[FindWhere],
    connects: Vec<UpdateSet>,
    disconnects: Vec<UpdateSet>,
    parents: &[M],
    client: &C,
) -> anyhow::Result<()>
where
    M: DinocoRelationValue,
    C: MutationExecutor,
{
    // `.connect_batch(...)`/`.disconnect_batch(...)` expand to one `UpdateSet`
    // per value up front (see `IntoUpdateSets`), so many-to-many connects and
    // disconnects targeting the same relation are grouped here and collapsed
    // into a single multi-row `INSERT`/IN-list `DELETE` instead of one query
    // per value.
    let mut many_to_many_connects: Vec<(ManyToManyUpdate, Vec<DinocoValue>)> = Vec::new();

    for connect in connects {
        let UpdateOperation::ConnectManyToMany(relation) = connect.operation else {
            let rows = connect_rows(conditions, connect.field, connect.value)?;

            if !rows.is_empty() {
                let fields = connect_fields(conditions, connect.field)?;
                let query = InsertQuery { table, fields, rows, returning: None };
                client.insert(query).await?;
            }
            continue;
        };

        match many_to_many_connects.iter_mut().find(|(existing, _)| *existing == relation) {
            Some((_, values)) => values.push(connect.value),
            None => many_to_many_connects.push((relation, vec![connect.value])),
        }
    }

    for (relation, children) in many_to_many_connects {
        let rows = many_to_many_connect_rows(parents, relation, children);

        if !rows.is_empty() {
            let query = InsertQuery {
                table: relation.join_table,
                fields: vec![relation.join_parent_field, relation.join_child_field],
                rows,
                returning: None,
            };
            client.insert(query).await?;
        }
    }

    let mut many_to_many_disconnects: Vec<(ManyToManyUpdate, Vec<DinocoValue>)> = Vec::new();

    for disconnect in disconnects {
        let UpdateOperation::DisconnectManyToMany(relation) = disconnect.operation else {
            for condition_group in disconnect_conditions(conditions, disconnect.field, disconnect.value)? {
                let query = DeleteQuery { table, conditions: condition_group, returning: None };
                client.delete(query).await?;
            }
            continue;
        };

        match many_to_many_disconnects.iter_mut().find(|(existing, _)| *existing == relation) {
            Some((_, values)) => values.push(disconnect.value),
            None => many_to_many_disconnects.push((relation, vec![disconnect.value])),
        }
    }

    for (relation, children) in many_to_many_disconnects {
        for source in many_to_many_source_values(parents, relation.parent_field) {
            let query = DeleteQuery {
                table: relation.join_table,
                conditions: vec![
                    FindWhere::Eq(relation.join_parent_field, source),
                    FindWhere::Batch(relation.join_child_field, children.clone()),
                ],
                returning: None,
            };
            client.delete(query).await?;
        }
    }

    Ok(())
}

fn many_to_many_connect_rows<M>(
    parents: &[M],
    relation: ManyToManyUpdate,
    children: Vec<DinocoValue>,
) -> Vec<Vec<DinocoValue>>
where
    M: DinocoRelationValue,
{
    let sources = many_to_many_source_values(parents, relation.parent_field);
    let mut rows = Vec::with_capacity(sources.len() * children.len());

    for parent in &sources {
        for child in &children {
            rows.push(vec![parent.clone(), child.clone()]);
        }
    }

    rows
}

fn many_to_many_source_values<M>(parents: &[M], parent_field: &'static str) -> Vec<DinocoValue>
where
    M: DinocoRelationValue,
{
    let mut values = Vec::new();
    for value in parents.iter().filter_map(|parent| parent.dinoco_relation_value(parent_field)) {
        if !values.contains(&value) {
            values.push(value);
        }
    }
    values
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
