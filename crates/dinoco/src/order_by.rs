use dinoco_engine::FindOrderBy;

pub struct OrderBy {
    name: &'static str,
}

impl OrderBy {
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }

    pub fn asc(self) -> FindOrderBy {
        FindOrderBy::Asc(self.name)
    }

    pub fn desc(self) -> FindOrderBy {
        FindOrderBy::Desc(self.name)
    }
}
