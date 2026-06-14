use dinoco::{
    IntoDinocoValue, Model, Projection, Rowable, ScalarField, insert_into, insert_many, update, update_many,
};

#[derive(Debug, Clone, Rowable)]
struct Event {
    id: dinoco::Uuid,
    sequence: dinoco::Snowflake,
    name: String,
}

struct EventWhere {
    id: ScalarField<String>,
    sequence: ScalarField<i64>,
    name: ScalarField<String>,
}

struct EventInclude {}

impl Projection<Event> for Event {
    fn columns() -> &'static [&'static str] {
        &["id", "sequence", "name"]
    }
}

impl dinoco::InsertModel for Event {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "sequence", "name"]
    }

    fn into_insert_row(self) -> Vec<dinoco::DinocoValue> {
        vec![self.id.into_dinoco_value(), self.sequence.into_dinoco_value(), self.name.into_dinoco_value()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco::Expression> {
        vec![dinoco::Expression::Column("id".to_string()).eq(self.id.clone().into_dinoco_value())]
    }
}

impl dinoco::UpdateModel for Event {
    fn update_columns() -> &'static [&'static str] {
        &["name"]
    }

    fn into_update_row(self) -> Vec<dinoco::DinocoValue> {
        vec![self.name.into_dinoco_value()]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco::Expression> {
        vec![dinoco::Expression::Column("id".to_string()).eq(self.id.clone().into_dinoco_value())]
    }
}

impl Model for Event {
    type Include = EventInclude;
    type Where = EventWhere;

    fn table_name() -> &'static str {
        "events"
    }
}

impl Default for EventWhere {
    fn default() -> Self {
        Self {
            id: ScalarField::new("id"),
            sequence: ScalarField::new("sequence"),
            name: ScalarField::new("name"),
        }
    }
}

impl Default for EventInclude {
    fn default() -> Self {
        Self {}
    }
}

fn main() {
    let first = Event { id: dinoco::Uuid::new(), sequence: dinoco::Snowflake::from(1), name: "One".to_string() };
    let second = Event { id: dinoco::Uuid::new(), sequence: dinoco::Snowflake::from(2), name: "Two".to_string() };
    let first_id_as_string = first.id.to_string();
    let first_sequence_as_i64 = first.sequence.as_i64();

    let _ = insert_into::<Event>().values(&first);
    let _ = insert_many::<Event>().values(vec![&first, &second]);

    let _ = update::<Event>().cond(|x| x.id.eq(first.id.clone())).values(&first);
    let _ = update::<Event>().cond(|x| x.id.eq(first_id_as_string)).values(&first);
    let _ = update::<Event>().cond(|x| x.sequence.eq(first.sequence)).values(&first);
    let _ = update::<Event>().cond(|x| x.sequence.eq(first_sequence_as_i64)).values(&first);
    let _ = update_many::<Event>().cond(|x| x.sequence.gt(0_i64)).values(vec![&first, &second]);
}
