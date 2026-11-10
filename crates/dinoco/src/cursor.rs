//! Keyset (cursor) pagination for `find_many`: `.after(...)` and
//! `.before(...)` keep the rows that come after or before a cursor row in the
//! query's `order_by` order.
//!
//! The cursor is turned into plain conditions on the `order_by` columns, so it
//! compiles the same in every database and combines with any filter:
//!
//! ```text
//! order_by(created_at DESC, id DESC) + after((t, i))
//!   => created_at <= t AND (created_at < t OR (created_at = t AND id < i))
//! ```

use dinoco_engine::{DinocoValue, FindOrderBy, FindQuery, FindWhere, OrderByList};

use crate::{DinocoRelationValue, IncludeApplier, IncludeLoader, IncludeLoaderFuture, ReadTarget};

/// The values of a row's `order_by` columns, in the order the query sorts by
/// them, which `find_many(...).after(...)` and `.before(...)` page from.
///
/// Build one from values a client sent back (`Cursor::new([...])`), or pass a
/// row, or a tuple of values, to `after`/`before` directly.
///
/// ```ignore
/// let page = find_many::<Post>()
///     .order_by(|post| post.created_at.desc())
///     .order_by(|post| post.id.desc())
///     .after(Cursor::new([DinocoValue::from(created_at), DinocoValue::from(id)]))
///     .take(20)
///     .execute(&client)
///     .await?;
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Cursor {
    values: Vec<DinocoValue>,
}

impl Cursor {
    pub fn new<I, V>(values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: Into<DinocoValue>,
    {
        Self { values: values.into_iter().map(Into::into).collect() }
    }

    /// The value of each `order_by` column, in order.
    pub fn values(&self) -> &[DinocoValue] {
        &self.values
    }

    pub fn into_values(self) -> Vec<DinocoValue> {
        self.values
    }
}

/// What `.after(...)` and `.before(...)` take: a row ([`DinocoRelationValue`],
/// every model and `select` projection), whose `order_by` columns are read when
/// the call is made, a [`Cursor`], or a tuple of up to four values in
/// `order_by` order.
pub trait IntoCursor {
    /// The cursor's values for the `order_by` columns known so far, or why
    /// there are none.
    #[doc(hidden)]
    fn into_cursor_values(self, order_by: &[FindOrderBy]) -> Result<Vec<DinocoValue>, String>;
}

impl<S> IntoCursor for &S
where
    S: DinocoRelationValue,
{
    fn into_cursor_values(self, order_by: &[FindOrderBy]) -> Result<Vec<DinocoValue>, String> {
        if order_by.is_empty() {
            return Err("call `order_by` before passing a row to `after`/`before`: the row's values are read for \
                        the columns ordered by so far"
                .to_string());
        }

        order_by
            .iter()
            .map(|order| {
                self.dinoco_relation_value(order.field()).ok_or_else(|| {
                    format!(
                        "the cursor row has no value for `{}`: it is NULL or the row does not select it",
                        order.field()
                    )
                })
            })
            .collect()
    }
}

impl IntoCursor for Cursor {
    fn into_cursor_values(self, _order_by: &[FindOrderBy]) -> Result<Vec<DinocoValue>, String> {
        Ok(self.values)
    }
}

macro_rules! cursor_tuples {
    ($(($($value:ident: $ty:ident),+)),* $(,)?) => {
        $(
            impl<$($ty),+> IntoCursor for ($($ty,)+)
            where
                $($ty: Into<DinocoValue>,)+
            {
                fn into_cursor_values(self, _order_by: &[FindOrderBy]) -> Result<Vec<DinocoValue>, String> {
                    let ($($value,)+) = self;
                    Ok(vec![$($value.into()),+])
                }
            }
        )*
    };
}

cursor_tuples!((a: A), (a: A, b: B), (a: A, b: B, c: C), (a: A, b: B, c: C, d: D));

/// A cursor set on a builder: its values, or why there are none, and which
/// side of the cursor row is kept. Boxed on the builder so plain finds stay
/// as small as before.
#[doc(hidden)]
#[derive(Debug)]
pub struct KeysetCursor {
    values: Result<Vec<DinocoValue>, String>,
    before: bool,
}

impl KeysetCursor {
    pub(crate) fn new(cursor: impl IntoCursor, order_by: &OrderByList, before: bool) -> Box<Self> {
        Box::new(Self { values: cursor.into_cursor_values(order_by.as_slice()), before })
    }

    /// Adds the cursor's condition to `query` as its last condition. With
    /// `flip` and a `before` cursor, the order is reversed so `take` keeps the
    /// rows right before the cursor; the caller then reverses the rows read
    /// back. Returns whether it did.
    pub(crate) fn apply(self, query: &mut FindQuery, flip: bool) -> anyhow::Result<bool> {
        let orders = query.order_by.as_slice();
        if orders.is_empty() {
            anyhow::bail!(
                "`after`/`before` on `{}` needs an `order_by`: the cursor is a position in that order",
                query.from
            );
        }

        let values = self.values.map_err(|message| anyhow::anyhow!("invalid cursor on `{}`: {message}", query.from))?;
        if values.len() != orders.len() {
            anyhow::bail!(
                "the cursor on `{}` has {} value(s) but the query orders by {} column(s); pass one value per \
                 `order_by` column, in order",
                query.from,
                values.len(),
                orders.len()
            );
        }
        if let Some(position) = values.iter().position(|value| matches!(value, DinocoValue::Null)) {
            anyhow::bail!(
                "the cursor value for `{}` on `{}` is NULL; keyset pagination needs non-null `order_by` columns",
                orders[position].field(),
                query.from
            );
        }

        let condition = keyset_condition(orders, values, self.before);
        query.conditions.push(condition);

        if !(flip && self.before) {
            return Ok(false);
        }

        query.order_by = query
            .order_by
            .as_slice()
            .iter()
            .map(|order| match *order {
                FindOrderBy::Asc(field) => FindOrderBy::Desc(field),
                FindOrderBy::Desc(field) => FindOrderBy::Asc(field),
            })
            .collect();
        Ok(true)
    }
}

/// The rows strictly after (or before) `values` in the order of `orders`:
/// `c1 > v1 OR (c1 = v1 AND c2 > v2) OR ...`, each comparison flipped for a
/// descending column. With more than one column the first comparison is
/// repeated inclusively in front, so an index on the columns can seek to it.
fn keyset_condition(orders: &[FindOrderBy], values: Vec<DinocoValue>, before: bool) -> FindWhere {
    let past = |order: &FindOrderBy, value: DinocoValue| {
        let field = order.field();
        // `after` an ascending column keeps greater values.
        if order.is_descending() == before { FindWhere::Gt(field, value) } else { FindWhere::Lt(field, value) }
    };

    if values.len() == 1 {
        let value = values.into_iter().next().expect("one cursor value");
        return past(&orders[0], value);
    }

    let mut branches = Vec::with_capacity(values.len());
    for position in 0..values.len() {
        let mut branch = Vec::with_capacity(position + 1);
        for (order, value) in orders[..position].iter().zip(&values) {
            branch.push(FindWhere::Eq(order.field(), value.clone()));
        }
        branch.push(past(&orders[position], values[position].clone()));
        branches.push(if branch.len() == 1 { branch.pop().expect("one condition") } else { FindWhere::And(branch) });
    }

    let first = &orders[0];
    let seek = if first.is_descending() == before {
        FindWhere::Gte(first.field(), values[0].clone())
    } else {
        FindWhere::Lte(first.field(), values[0].clone())
    };

    FindWhere::And(vec![seek, FindWhere::Or(branches)])
}

/// What a find still does with its rows after a cursor is applied, run as the
/// last of its includes so the find's own future stays as it was without a
/// cursor: reverse the rows read for a `before` cursor, or fail with the
/// cursor's error (the find then reads no row).
enum CursorStep {
    Reverse,
    Fail(String),
}

impl<S> IncludeLoader<S> for CursorStep
where
    S: Send + 'static,
{
    fn load_applier<'a>(&'a self, _target: &'a ReadTarget<'a>, _parents: &'a [S]) -> IncludeLoaderFuture<'a, S> {
        let result = match self {
            CursorStep::Reverse => Ok(Box::new(|rows: &mut [S]| rows.reverse()) as IncludeApplier<S>),
            CursorStep::Fail(message) => Err(anyhow::Error::msg(message.clone())),
        };

        Box::pin(std::future::ready(result))
    }
}

/// Applies the cursor of a find to its query, as its last condition, and
/// adds the [`CursorStep`] it needs, if any, after its includes.
pub(crate) fn apply_cursor_to_find<S>(
    cursor: Option<Box<KeysetCursor>>,
    query: &mut FindQuery,
    includes: &mut Vec<Box<dyn IncludeLoader<S>>>,
) where
    S: Send + 'static,
{
    let Some(cursor) = cursor else {
        return;
    };

    match cursor.apply(query, true) {
        Ok(false) => {}
        Ok(true) => includes.push(Box::new(CursorStep::Reverse)),
        Err(error) => {
            // The cursor's condition is always the last one (a counting find
            // leaves it out of the count): here one that keeps every row.
            query.conditions.push(FindWhere::And(Vec::new()));
            query.limit = 0;
            includes.push(Box::new(CursorStep::Fail(error.to_string())));
        }
    }
}
