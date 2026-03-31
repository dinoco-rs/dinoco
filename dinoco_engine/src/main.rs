use dinoco_engine::{
    AlterTableStatement, ColumnDefinition, ColumnType, ConstraintDefinition, CreateTableStatement, DinocoAdapter, DinocoValue, DropTableStatement, Filterable, PostgresAdapter,
    SelectStatement, col,
};

#[tokio::main]
async fn main() {
    let adapter = PostgresAdapter::connect("postgresql://postgres.votzhpldwahwltnonmnn:CriadorDeBot@aws-1-sa-east-1.pooler.supabase.com:5432/postgres".to_string())
        .await
        .unwrap();

    // let query = SelectStatement::new(adapter.dialect())
    //     .select(&["id", "name"])
    //     .from("users")
    //     .condition(col("name").eq("Matheus"))
    //     .condition(col("age").gte(10))
    //     .to_sql();

    // println!("{:?}", query);

    // Exemplo 1: Deletar tabela simples
    let stmt = AlterTableStatement::new(adapter.dialect(), "pedidos")
        .add_constraint(ConstraintDefinition::unique("uk_codigo_pedido", vec!["codigo"]))
        .add_constraint(ConstraintDefinition::foreign_key("fk_usuario", vec!["usuario_id"], "usuarios", vec!["id"]).on_delete("CASCADE"))
        .drop_constraint("chk_valor_minimo");

    let queries = stmt.to_sql();

    for (sql, params) in queries {
        println!("{}", sql);
    }
}
