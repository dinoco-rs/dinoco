use dinoco_engine::{DinocoGenericRow, DinocoResult, DinocoRow};

mod insert;
mod lookup;
mod qualify;
mod read;
mod relation;
mod reload;
mod update;

pub use insert::*;
pub use qualify::*;
pub use read::*;
pub use relation::*;
pub(crate) use reload::{execute_reload_by_identity, execute_reload_many_by_identity};
pub use update::*;

pub(super) struct DinocoCountRow {
    pub(super) count: i64,
}

pub(super) struct DinocoValueRow {
    pub(super) value: dinoco_engine::DinocoValue,
}

pub(super) struct DinocoPairRow {
    pub(super) left: dinoco_engine::DinocoValue,
    pub(super) right: dinoco_engine::DinocoValue,
}

impl DinocoRow for DinocoCountRow {
    fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
        Ok(Self { count: row.get(0)? })
    }
}

impl DinocoRow for DinocoValueRow {
    fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
        Ok(Self { value: row.get_value(0)? })
    }
}

impl DinocoRow for DinocoPairRow {
    fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
        Ok(Self { left: row.get_value(0)?, right: row.get_value(1)? })
    }
}

pub(super) fn should_qualify_query_column(value: &str) -> bool {
    !value.is_empty()
        && value != "*"
        && !value.contains('.')
        && !value.contains(' ')
        && !value.contains('(')
        && !value.contains(')')
        && !value.contains(',')
}
