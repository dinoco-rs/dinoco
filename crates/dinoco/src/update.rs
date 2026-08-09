use std::marker::PhantomData;

use dinoco_engine::{
    DeleteQuery, DinocoClient, DinocoEntity, DinocoProjection, DinocoRowModel, DinocoValue, FindQuery, FindWhere,
    InsertQuery, ManyToManyUpdate, ManyToManyWriteQuery, UpdateOperation, UpdateSet,
};

use crate::DinocoRelationValue;

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
            UpdateOperation::Connect | UpdateOperation::ConnectManyToMany(_) => connects.push(set),
            UpdateOperation::Disconnect | UpdateOperation::DisconnectManyToMany(_) => disconnects.push(set),
        }
    }

    (updates, connects, disconnects)
}

pub(crate) fn has_many_to_many_update_sets(sets: &[UpdateSet]) -> bool {
    sets.iter().any(|set| {
        matches!(set.operation, UpdateOperation::ConnectManyToMany(_) | UpdateOperation::DisconnectManyToMany(_))
    })
}

pub(crate) fn transaction_many_to_many_writes(
    parent_table: &'static str,
    parent_conditions: &[FindWhere],
    connects: Vec<UpdateSet>,
    disconnects: Vec<UpdateSet>,
) -> anyhow::Result<(Vec<ManyToManyWriteQuery>, Vec<ManyToManyWriteQuery>)> {
    let connects = connects
        .into_iter()
        .map(|set| transaction_many_to_many_write(parent_table, parent_conditions, set, true))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let disconnects = disconnects
        .into_iter()
        .map(|set| transaction_many_to_many_write(parent_table, parent_conditions, set, false))
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok((connects, disconnects))
}

fn transaction_many_to_many_write(
    parent_table: &'static str,
    parent_conditions: &[FindWhere],
    set: UpdateSet,
    connect: bool,
) -> anyhow::Result<ManyToManyWriteQuery> {
    let relation = match set.operation {
        UpdateOperation::ConnectManyToMany(relation) if connect => relation,
        UpdateOperation::DisconnectManyToMany(relation) if !connect => relation,
        UpdateOperation::Connect | UpdateOperation::Disconnect => {
            anyhow::bail!(
                "transaction relation writes require an implicit many-to-many virtual key; '{}' is a materialized field",
                set.field,
            )
        }
        _ => anyhow::bail!("transaction relation write '{}' has an unexpected operation", set.field),
    };

    Ok(ManyToManyWriteQuery {
        parent_table,
        join_table: relation.join_table,
        parent_field: relation.parent_field,
        join_parent_field: relation.join_parent_field,
        join_child_field: relation.join_child_field,
        child_value: set.value,
        parent_conditions: parent_conditions.to_vec(),
    })
}

pub(crate) async fn load_update_matches<M, S>(conditions: &[FindWhere], client: &DinocoClient) -> anyhow::Result<Vec<S>>
where
    M: DinocoEntity,
    S: DinocoProjection<M> + DinocoRowModel,
{
    let mut query = FindQuery::new(S::FIELDS, M::TABLE_NAME, -1, -1);
    query.conditions = conditions.to_vec();
    client.backend.query::<S>(query).await
}

pub(crate) async fn execute_relation_update_sets<M>(
    table: &'static str,
    conditions: &[FindWhere],
    connects: Vec<UpdateSet>,
    disconnects: Vec<UpdateSet>,
    parents: &[M],
    client: &DinocoClient,
) -> anyhow::Result<()>
where
    M: DinocoRelationValue,
{
    for connect in connects {
        if let UpdateOperation::ConnectManyToMany(relation) = connect.operation {
            let rows = many_to_many_connect_rows(parents, relation, connect.value);
            if !rows.is_empty() {
                let query = InsertQuery {
                    table: relation.join_table,
                    fields: vec![relation.join_parent_field, relation.join_child_field],
                    rows,
                    returning: None,
                };
                client.backend.insert(query).await?;
            }
            continue;
        }

        let rows = connect_rows(conditions, connect.field, connect.value)?;

        if !rows.is_empty() {
            let fields = connect_fields(conditions, connect.field)?;
            let query = InsertQuery { table, fields, rows, returning: None };
            client.backend.insert(query).await?;
        }
    }

    for disconnect in disconnects {
        if let UpdateOperation::DisconnectManyToMany(relation) = disconnect.operation {
            for source in many_to_many_source_values(parents, relation.parent_field) {
                let query = DeleteQuery {
                    table: relation.join_table,
                    conditions: vec![
                        FindWhere::Eq(relation.join_parent_field, source),
                        FindWhere::Eq(relation.join_child_field, disconnect.value.clone()),
                    ],
                    returning: None,
                };
                client.backend.delete(query).await?;
            }
            continue;
        }

        for condition_group in disconnect_conditions(conditions, disconnect.field, disconnect.value)? {
            let query = DeleteQuery { table, conditions: condition_group, returning: None };
            client.backend.delete(query).await?;
        }
    }

    Ok(())
}

fn many_to_many_connect_rows<M>(parents: &[M], relation: ManyToManyUpdate, child: DinocoValue) -> Vec<Vec<DinocoValue>>
where
    M: DinocoRelationValue,
{
    many_to_many_source_values(parents, relation.parent_field)
        .into_iter()
        .map(|parent| vec![parent, child.clone()])
        .collect()
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
