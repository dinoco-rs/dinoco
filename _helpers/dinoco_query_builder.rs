// dinoco_query_builder.rs
// Exemplo completo com suporte a múltiplos campos no SELECT usando tuples

use std::marker::PhantomData;

// -------------------- MODEL --------------------

pub struct User {
    pub id: i32,
    pub name: String,
}

// Phantom fields
pub struct UserFields;

pub struct UserIdCol;
pub struct UserNameCol;

// Column trait
pub trait Column {
    const NAME: &'static str;
    type Type;
}

impl Column for UserIdCol {
    const NAME: &'static str = "id";
    type Type = i32;
}

impl Column for UserNameCol {
    const NAME: &'static str = "name";
    type Type = String;
}

// Accessor estilo Prisma
impl UserFields {
    pub const id: UserIdCol = UserIdCol;
    pub const name: UserNameCol = UserNameCol;
}

// -------------------- SELECTABLE --------------------

pub trait Selectable {
    type Output;
    fn columns() -> String;
}

// Single column
impl<C> Selectable for C
where
    C: Column,
{
    type Output = C::Type;

    fn columns() -> String {
        C::NAME.to_string()
    }
}

// Macro para tuples
macro_rules! impl_selectable_tuple {
    ($($name:ident),+) => {
        impl<$($name),+> Selectable for ($($name,)+)
        where
            $($name: Column),+
        {
            type Output = ($($name::Type,)+);

            fn columns() -> String {
                let mut cols = Vec::new();
                $( cols.push($name::NAME); )+
                cols.join(", ")
            }
        }
    };
}

// Implementações
impl_selectable_tuple!(A, B);
impl_selectable_tuple!(A, B, C);
impl_selectable_tuple!(A, B, C, D);

// -------------------- QUERY BUILDER --------------------

pub struct QueryBuilder<Model, S> {
    _marker: PhantomData<(Model, S)>,
    bindings: Vec<String>,
    where_clauses: Vec<&'static str>,
}

pub fn find_many<T>() -> QueryBuilder<T, ()> {
    QueryBuilder {
        _marker: PhantomData,
        bindings: Vec::new(),
        where_clauses: Vec::new(),
    }
}

impl<Model, S> QueryBuilder<Model, S> {
    pub fn select<NewS, F>(self, _f: F) -> QueryBuilder<Model, NewS>
    where
        F: FnOnce(UserFields) -> NewS,
        NewS: Selectable,
    {
        QueryBuilder {
            _marker: PhantomData,
            bindings: self.bindings,
            where_clauses: self.where_clauses,
        }
    }
}

// -------------------- USER IMPLEMENTATION --------------------

impl<S> QueryBuilder<User, S>
where
    S: Selectable,
{
    pub fn where_name(mut self, name: impl Into<String>) -> Self {
        self.where_clauses.push("name = ?");
        self.bindings.push(name.into());
        self
    }

    pub async fn execute(self) -> Vec<S::Output> {
        let mut sql = String::new();

        sql.push_str("SELECT ");
        sql.push_str(&S::columns());

        sql.push_str(" FROM users");

        if !self.where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.where_clauses.join(" AND "));
        }

        println!("SQL: {}", sql);
        println!("Bindings: {:?}", self.bindings);

        vec![]
    }
}

// -------------------- USO --------------------

async fn example() {
    let _users = find_many::<User>()
        .select(|u| (u.id, u.name))
        .where_name("Matheus")
        .execute()
        .await;

    // Retorno inferido:
    // Vec<(i32, String)>
}
