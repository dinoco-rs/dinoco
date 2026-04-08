use dinoco::{
    DinocoError, DinocoResult, DinocoValue, InsertModel, Model, Projection, Rowable, ScalarField, UpdateModel,
};

#[derive(Debug, Clone, Rowable)]
pub struct User {
    pub id: i64,
    pub name: String,
}

pub struct UserWhere {
    pub id: ScalarField<i64>,
    pub name: ScalarField<String>,
}

pub struct UserInclude {}

#[derive(Debug, Clone, Rowable)]
pub struct UserSummary {
    pub id: i64,
}

#[derive(Debug, Clone, Rowable)]
pub struct Team {
    pub id: String,
    pub name: String,
}

pub struct TeamWhere {
    pub id: ScalarField<String>,
    pub name: ScalarField<String>,
}

pub struct TeamInclude {}

impl Projection<User> for User {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl Projection<User> for UserSummary {
    fn columns() -> &'static [&'static str] {
        &["id"]
    }
}

impl Projection<Team> for Team {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl InsertModel for User {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }

    fn validate_insert(&self) -> DinocoResult<()> {
        if self.name.trim().is_empty() {
            return Err(DinocoError::ParseError("User.name cannot be empty".to_string()));
        }

        Ok(())
    }
}

impl InsertModel for Team {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "name"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.name.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id.clone())]
    }
}

impl UpdateModel for User {
    fn update_columns() -> &'static [&'static str] {
        &["name"]
    }

    fn into_update_row(self) -> Vec<DinocoValue> {
        vec![self.name.into()]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl UpdateModel for Team {
    fn update_columns() -> &'static [&'static str] {
        &["name"]
    }

    fn into_update_row(self) -> Vec<DinocoValue> {
        vec![self.name.into()]
    }

    fn update_identity_conditions(&self) -> Vec<dinoco_engine::Expression> {
        vec![dinoco_engine::Expression::Column("id".to_string()).eq(self.id.clone())]
    }
}

impl Model for User {
    type Include = UserInclude;
    type Where = UserWhere;

    fn table_name() -> &'static str {
        "users"
    }
}

impl Model for Team {
    type Include = TeamInclude;
    type Where = TeamWhere;

    fn table_name() -> &'static str {
        "teams"
    }
}

impl Default for UserWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for UserInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for TeamWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for TeamInclude {
    fn default() -> Self {
        Self {}
    }
}
