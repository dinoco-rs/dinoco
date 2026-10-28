use std::future::Future;
use std::marker::PhantomData;

use dinoco_engine::{DinocoEntity, FindWhere, WhereComplex};

use crate::{CountLoader, FilterTarget, IntoCountLoader, ReadExecutor, execute_count};

pub struct Count<M>
where
    M: DinocoEntity,
{
    conditions: Vec<FindWhere>,
    complex_where: bool,
    counts: Vec<Box<dyn CountLoader<M::Count>>>,
    marker: PhantomData<M>,
}

pub fn count<M>() -> Count<M>
where
    M: DinocoEntity,
{
    Count::with_conditions(Vec::new())
}

impl<M> Count<M>
where
    M: DinocoEntity,
{
    /// A count that starts from `conditions`, as if each came from a `where_`.
    pub(crate) fn with_conditions(conditions: Vec<FindWhere>) -> Self {
        Count { conditions, complex_where: false, counts: Vec::new(), marker: PhantomData }
    }

    pub(crate) fn into_parts(self) -> (Vec<FindWhere>, Vec<Box<dyn CountLoader<M::Count>>>) {
        (self.conditions, self.counts)
    }
}

impl<M> Count<M>
where
    M: DinocoEntity,
    M::Count: 'static,
{
    pub fn where_<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> FindWhere,
    {
        if !self.complex_where {
            self.conditions.push(callback(M::Where::default()));
        }

        self
    }

    pub fn where_complex<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where, WhereComplex) -> FindWhere,
    {
        self.conditions = vec![callback(M::Where::default(), WhereComplex)];
        self.complex_where = true;

        self
    }

    /// Drops the conditions on the field `callback` returns: every `where_`
    /// that filters on it and, in a `where_complex` tree, every member of an
    /// `and` group that does. An `or` or `not` group that mentions the field
    /// goes as a whole, so removing never narrows the count. A relation field
    /// drops its relation filters. Made for
    /// `find_many(...).count_transform(...)`, which starts from the find's
    /// filters.
    pub fn remove<F, T>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::Where) -> T,
        T: FilterTarget,
    {
        remove_conditions(&mut self.conditions, &callback(M::Where::default()));

        self
    }

    pub fn includes<F, I>(mut self, callback: F) -> Self
    where
        F: FnOnce(M::CountInclude) -> I,
        I: IntoCountLoader<M, M::Count>,
    {
        self.counts.push(callback(M::CountInclude::default()).into_count_loader());

        self
    }

    pub fn execute<C>(self, client: C) -> impl Future<Output = anyhow::Result<M::Count>>
    where
        C: ReadExecutor,
        M::Count: crate::DinocoCountModel<M>,
    {
        execute_count::<M>(self.conditions, self.counts, client)
    }
}

/// Drops the conditions joined by `AND` that mention `field`, going into
/// `and` groups and dropping the ones left empty.
fn remove_conditions(conditions: &mut Vec<FindWhere>, field: &impl FilterTarget) {
    conditions.retain_mut(|condition| match condition {
        FindWhere::And(group) => {
            remove_conditions(group, field);
            !group.is_empty()
        }
        condition => !mentions(condition, field),
    });
}

fn mentions(condition: &FindWhere, field: &impl FilterTarget) -> bool {
    match condition {
        FindWhere::And(group) | FindWhere::Or(group) => group.iter().any(|condition| mentions(condition, field)),
        FindWhere::Not(condition) => mentions(condition, field),
        condition => field.filters(condition),
    }
}
