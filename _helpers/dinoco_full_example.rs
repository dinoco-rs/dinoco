// dinoco_full_example.rs
// Exemplos:
// 1. Sem select (retorna Model completo)
// 2. Com select (retorna tupla tipada)
// 3. Simulação de execução + mapeamento

use std::marker::PhantomData;

// -------------------- MODEL --------------------

#[derive(Debug)]
pub struct User {
    pub id: i32,
    pub name: String,
}

// -------------------- COLUMNS --------------------

pub struct UserFields;

pub struct UserIdCol;
pub struct UserNameCol;

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

impl UserFields {
    pub const id: UserIdCol = UserIdCol;
    pub const name: UserNameCol = UserNameCol;
}

// -------------------- SELECTABLE --------------------

pub trait Selectable {
    type Output;
    fn columns() -> String;
}

// Default = Model inteiro
impl Selectable for () {
    type Output = User;

    fn columns() -> String {
        "id, name".to_string()
    }
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

// Tuples
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

impl_selectable_tuple!(A, B);
impl_selectable_tuple!(A, B, C);

// -------------------- QUERY BUILDER --------------------

pub struct QueryBuilder<Model, S> {
    _marker: PhantomData<(Model, S)>,
    where_clauses: Vec<&'static str>,
    bindings: Vec<String>,
}

pub fn find_many<T>() -> QueryBuilder<T, ()> {
    QueryBuilder {
        _marker: PhantomData,
        where_clauses: vec![],
        bindings: vec![],
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
            where_clauses: self.where_clauses,
            bindings: self.bindings,
        }
    }
}

// -------------------- IMPLEMENTAÇÃO USER --------------------

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

        // ---------------- MOCK DB ----------------
        // Simulando retorno do banco:
        let fake_rows = vec![
            (1, "Matheus".to_string()),
            (2, "João".to_string()),
        ];

        // ---------------- MAPEAMENTO ----------------
        // Aqui normalmente você usaria um driver real (tokio-postgres, etc)

        // Caso 1: retorno completo
        if std::any::TypeId::of::<S::Output>() == std::any::TypeId::of::<User>() {
            let mapped: Vec<User> = fake_rows
                .into_iter()
                .map(|(id, name)| User { id, name })
                .collect();

            // hack só pra exemplo
            return unsafe { std::mem::transmute(mapped) };
        }

        // Caso 2: tupla (id, name)
        let mapped = fake_rows;

        unsafe { std::mem::transmute(mapped) }
    }
}

// -------------------- EXEMPLOS --------------------

async fn main_example() {
    // 1. SEM SELECT (retorna model completo)
    let users: Vec<User> = find_many::<User>()
        .condition(|x| x.name.equals("Matheus"))
		.select(|u| (u.id, u.name))
		.include(|u| u.posts().condition(|x| x.likes.qt(20)).include(|x| x.comments()))
		.include(|u| u.profits())
		.include(|u| u.profits())
		.take(20)
		.orderBy(|x| x.createdAt.desc())
        .execute()
        .await;

    println!("Sem select: {:?}", users);

    // 2. COM SELECT (retorna tupla)
    let users_tuple: Vec<(i32, String)> = find_many::<User>()
        .select(|u| (u.id, u.name))
        .where_name("Matheus")
        .execute()
        .await;

    println!("Com select: {:?}", users_tuple);
}
