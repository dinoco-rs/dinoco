use dinoco_engine::{OrderDirection, SelectStatement};

#[derive(Debug, Clone)]
pub struct IncludeNode {
    pub name: &'static str,
    pub statement: Option<SelectStatement>,
    pub includes: Vec<IncludeNode>,
}

#[derive(Debug, Clone, Copy)]
pub struct OrderBy {
    pub column: &'static str,
    pub direction: OrderDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadMode {
    ReplicaPreferred,
    Primary,
}
