pub trait QueryDialect {
    fn build_create_table<'a>(&self, stmt: &CreateTableStatement<'a, Self>, builder: &mut SqlBuilder<Self>)
    where
        Self: Sized;
}

impl QueryDialect for PostgresDialect {
    fn build_create_table<'a>(&self, stmt: &CreateTableStatement<'a, Self>, builder: &mut SqlBuilder<Self>) {
        // usa a lógica padrão + customizações
    }
}

impl QueryDialect for MySqlDialect {
    fn build_create_table<'a>(&self, stmt: &CreateTableStatement<'a, Self>, builder: &mut SqlBuilder<Self>) {
        // override só o que muda (ex: AUTO_INCREMENT)
    }
}
