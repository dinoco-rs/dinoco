// dinoco_dx_orm.rs
// Exemplo completo: DX estilo Prisma + type-safe + quase zero-cost

use std::marker::PhantomData;
use std::ops::Add;

// -------------------- MODEL --------------------

#[derive(Debug)]
pub struct User {
    pub id: i32,
    pub name: String,
}

// -------------------- COLUMN TRAIT --------------------

pub trait Column {
    const NAME: &'static str;
    type Type;
}

// -------------------- COL WRAPPER --------------------

pub struct Col<C: Column>(PhantomData<C>);

impl<C: Column> Copy for Col<C> {}
impl<C: Column> Clone for Col<C> {
    fn clone(&self) -> Self {
        *self
    }
}

// -------------------- USER COLUMNS --------------------

pub struct UserIdCol;
pub struct UserNameCol;

impl Column for UserIdCol {
    const NAME: &'static str = "id";
    type Type = i32;
}

impl Column for UserNameCol {
    const NAME: &'static str = "name";
    type Type = String;
}

// -------------------- FIELDS --------------------

pub struct UserFields;

impl UserFields {
    pub const id: Col<UserIdCol> = Col(PhantomData);
    pub const name: Col<UserNameCol> = Col(PhantomData);
}

// -------------------- SELECT LIST --------------------

pub struct SelectList<A, B>(PhantomData<(A, B)>);

// Col + Col
impl<A, B> Add<Col<B>> for Col<A>
where
    A: Column,
    B: Column,
{
    type Output = SelectList<A, B>;

    fn add(self, _rhs: Col<B>) -> Self::Output {
        SelectList(PhantomData)
    }
}

// SelectList + Col
impl<A, B, C> Add<Col<C>> for SelectList<A, B>
where
    A: Column,
    B: Column,
    C: Column,
{
    type Output = SelectList<(A, B), C>;

    fn add(self, _rhs: Col<C>) -> Self::Output {
        SelectList(PhantomData)
    }
}

// -------------------- SELECTABLE --------------------

pub trait Selectable {
    type Output;
    fn columns() -> &'static str;
}

// Default (model inteiro)
impl Selectable for () {
    type Output = User;

    fn columns() -> &'static str {
        "id, name"
    }
}

// Single column
impl<C: Column> Selectable for Col<C> {
    type Output = C::Type;

    fn columns() -> &'static str {
        C::NAME
    }
}

// 2 colunas
impl<A: Column, B: Column> Selectable for SelectList<A, B> {
    type Output = (A::Type, B::Type);

    fn columns() -> &'static str {
        // em produção isso viria via codegen
        "id, name"
    }
}

// -------------------- WHERE (Expression) --------------------

pub struct Eq<C: Column> {
    pub value: C::Type,
    _marker: PhantomData<C>,
}

impl<C: Column> Col<C> {
    pub fn eq(self, value: C::Type) -> Eq<C> {
        Eq {
            value,
            _marker: PhantomData,
        }
    }
}

// -------------------- QUERY BUILDER --------------------

pub struct QueryBuilder<Model, S> {
    _marker: PhantomData<(Model, S)>,
    where_sql: Option<&'static str>,
    bindings: Vec<String>,
}

pub fn find_many<T>() -> QueryBuilder<T, ()> {
    QueryBuilder {
        _marker: PhantomData,
        where_sql: None,
        bindings: vec![],
    }
}

impl<Model, S> QueryBuilder<Model, S> {
    pub fn select<NewS, F>(self, f: F) -> QueryBuilder<Model, NewS>
    where
        F: FnOnce(UserFields) -> NewS,
        NewS: Selectable,
    {
        let _ = f(UserFields);

        QueryBuilder {
            _marker: PhantomData,
            where_sql: self.where_sql,
            bindings: self.bindings,
        }
    }
}

// WHERE
impl<S> QueryBuilder<User, S>
where
    S: Selectable,
{
    pub fn where_<C: Column>(mut self, expr: Eq<C>) -> Self {
        self.where_sql = Some(match C::NAME {
            "id" => "id = ?",
            "name" => "name = ?",
            _ => unreachable!(),
        });

        self.bindings.push(format!("{:?}", expr.value));
        self
    }

    pub async fn execute(self) -> Vec<S::Output> {
        let mut sql = String::new();

        sql.push_str("SELECT ");
        sql.push_str(S::columns());
        sql.push_str(" FROM users");

        if let Some(w) = self.where_sql {
            sql.push_str(" WHERE ");
            sql.push_str(w);
        }

        println!("SQL: {}", sql);
        println!("Bindings: {:?}", self.bindings);

        // ---------------- MOCK DB ----------------
        let fake_rows = vec![
            (1, "Matheus".to_string()),
            (2, "João".to_string()),
        ];

        // ---------------- MAPEAMENTO ----------------
        if std::any::TypeId::of::<S::Output>() == std::any::TypeId::of::<User>() {
            let mapped: Vec<User> = fake_rows
                .into_iter()
                .map(|(id, name)| User { id, name })
                .collect();

            return unsafe { std::mem::transmute(mapped) };
        }

        let mapped = fake_rows;
        unsafe { std::mem::transmute(mapped) }
    }
}

// -------------------- EXEMPLOS --------------------

async fn main_example() {
    // 1. SEM SELECT
    let users: Vec<User> = find_many::<User>()
        .where_(UserFields::id.eq(1))
        .execute()
        .await;

    println!("Sem select: {:?}", users);

    // 2. COM SELECT (DX estilo Prisma)
    let users2: Vec<(i32, String)> = find_many::<User>()
        .select(|u| u.id + u.name)
        .where_(UserFields::name.eq("Matheus".to_string()))
        .execute()
        .await;

    println!("Com select: {:?}", users2);
}
