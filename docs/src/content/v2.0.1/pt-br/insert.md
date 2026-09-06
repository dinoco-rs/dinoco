# Insert

O Dinoco usa a própria entity gerada como entrada de insert — não existe uma derive `#[insertable]` separada, nenhuma "create struct" paralela, e nenhuma chamada `.with_relation(...)` para aprender. Defaults, IDs gerados e metadados de relação já vêm embutidos na entity que você já tem.

## Crie uma entity

Use o construtor `new()` gerado, depois ajuste qualquer field público que precisar antes de inserir:

```rust
let mut user = User::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);

user.bio = Some("Rust developer".to_string());
```

`new()` só pede valores escalares obrigatórios que não têm default nem gerador — veja [Models e fields](/pt-br/docs/orm/guide/models#a-funcao-new-gerada) para exatamente como essa lista de parâmetros é escolhida. Isso mantém a construção curta sem nunca esconder quais valores são de fato responsabilidade de quem chama.

## Insira uma entity

```rust
dinoco::insert_into::<User>()
    .values(&user)
    .execute(&client)
    .await?;
```

Passar `&user` empresta o valor — o insert não descarta nem move o seu valor, então você pode continuar usando `user` depois. Um valor owned também funciona, se você não precisar dele de novo.

## Insira várias entities

```rust
let users = vec![
    User::new("ana@example.com".to_string(), "Ana".to_string()),
    User::new("leo@example.com".to_string(), "Leo".to_string()),
];

dinoco::insert_many::<User>()
    .values(&users)
    .execute(&client)
    .await?;
```

Por baixo dos panos, isso agrupa cada row na instrução de insert multi-row nativa do adapter ativo — é um batch de verdade, não um loop disparando um `insert_into` por item.

## Insira relações

Preencha entities aninhadas diretamente no field de relação gerado. O mesmo padrão funciona para uma relação `Vec` (muitos) e para a relação `Option` (um):

```rust
let mut user = User::new(
    "ana@example.com".to_string(),
    "Ana".to_string(),
);
user.tokens = vec![UserToken::new(), UserToken::new()];
user.profile = Some(Profile::new());

dinoco::insert_into::<User>()
    .values(&user)
    .execute(&client)
    .await?;
```

O Dinoco insere dados aninhados de one-to-many, many-to-one e one-to-one usando os próprios metadados da relação, e propaga a chave gerada que o lado filho precisa — não tem nada extra para configurar além de preencher o field.

### Conecte many-to-many durante o insert

Numa relação many-to-many implícita, cada endpoint carrega um field virtual `Option<Id>` apontando para o model oposto. Preencha-o antes de chamar `insert_into`, e o endpoint é inserido *e* seu vínculo na pivô é criado, na mesma execução do builder:

```rust
let mut tag = Tag::new("documentation".to_string());
tag.task_id = Some(task.id.clone());

dinoco::insert_into::<Tag>()
    .values(&tag)
    .execute(&client)
    .await?;
```

A mesma regra se aplica de forma independente a cada item de um lote `insert_many`:

```rust
let mut tags = vec![
    Tag::new("rust".to_string()),
    Tag::new("database".to_string()),
];

for tag in &mut tags {
    tag.task_id = Some(task.id.clone());
}

dinoco::insert_many::<Tag>()
    .values(&tags)
    .execute(&client)
    .await?;
```

O Dinoco exclui `task_id` das colunas SQL de verdade de `tag` completamente. Depois de cada `Tag` ser inserida, ele usa esse valor virtual para inserir `(task.id, tag.id)` na tabela pivô separadamente. Deixar como `None` insere o endpoint sem vínculo nenhum — você sempre pode conectar depois. A alternativa, quando os dois endpoints já existem, é não preencher o field virtual em nenhum dos dois e chamar `connect` a partir da API de update.

> [!NOTE]
> O ID virtual é write-only, ponto final — ele volta `None` em toda leitura e todo insert com returning, incluindo `.returning::<S>()`. Não escreva código que espera inspecioná-lo depois para checar se um vínculo existe; consulte a relação em si em vez disso.

Os mesmos payloads funcionam dentro de um contexto de transaction também, fazendo o insert do endpoint e seu vínculo na pivô receberem commit ou rollback como uma única unidade:

```rust
dinoco::transaction(&client, |tx| async move {
    dinoco::insert_into::<Tag>().value(&tag).execute(tx).await?;
    dinoco::insert_many::<Tag>().values(&tags).execute(tx).await?;
    Ok(())
})
.await?;
```

Veja [Many-to-many implícito](/pt-br/docs/orm/guide/relations#many-to-many-implicito) para o quadro completo: fields gerados, comportamento da pivô, carregamento nos dois sentidos, counts, `connect`/`disconnect` e notas de migration.

## Identificadores gerados

Valores UUID e Snowflake são gerados pelo próprio Dinoco, do lado do client, antes mesmo das linhas de relação dependentes serem montadas — o que é exatamente o que torna possível inserir um pai e seus filhos aninhados numa única chamada, já que as linhas filhas podem referenciar a chave do pai antes dele sequer tocar no banco. Chaves de autoincremento funcionam ao contrário: elas são recuperadas *do* banco depois do insert, já que é o banco quem as gera.

## Retorne uma projeção

Sem `.returning(...)`, o resultado de sucesso de um insert é só `()` — não tem nada para devolver se você não pediu nada. Adicione um tipo de entity ou uma projeção `EntityExtend` quando quem chama de fato precisa dos dados inseridos:

```rust
let inserted = dinoco::insert_into::<User>()
    .values(&user)
    .returning::<UserSummary>()
    .execute(&client)
    .await?;

let inserted_many = dinoco::insert_many::<User>()
    .values(&users)
    .returning::<User>()
    .execute(&client)
    .await?;
```

A primeira chamada retorna um `UserSummary`; a segunda retorna `Vec<User>`. Só peça dados de returning quando você for realmente usá-los — um insert aninhado pode precisar de uma releitura extra internamente para montar a projeção exata que você pediu.
