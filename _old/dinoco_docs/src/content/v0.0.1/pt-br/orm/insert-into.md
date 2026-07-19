# insert_into

Usado para inserir um registro.

---

## O que você pode fazer

- Passar valores com `.values(item)`
- Inserir com relação via `.with_relation(related)`
- Conectar relação existente via `.with_connection(connected)`
- Executar com `.execute(&client)`

## Descrição dos métodos

- `.values(item)`: define o registro que será inserido.
- `.with_relation(related)`: insere um registro relacionado novo junto com o pai.
- `.with_connection(connected)`: conecta o registro inserido a um item já existente, normalmente em fluxos de relação suportados para conexão.
- `.returning::&lt;T&gt;()`: muda o retorno para uma projeção tipada do item inserido.
- `.execute(&client)`: executa a escrita no banco.

## Retorno

Sem `.returning::&lt;T&gt;()`, o retorno é:

```rust
DinocoResult<()>
```

Com `.returning::&lt;T&gt;()`, o retorno passa a ser:

```rust
DinocoResult<T>
```

## Exemplo simples

```rust
dinoco::insert_into::<User>()
    .values(User {
        id: "usr_1".to_string(),
        email: "ana@acme.com".to_string(),
        name: "Ana".to_string(),
    })
    .execute(&client)
    .await?;
```

## Exemplo com relação

Use `.with_relation(...)` quando o model gerado suporta inserir o pai e o relacionado juntos.

```rust
dinoco::insert_into::<User>()
    .values(user)
    .with_relation(profile)
    .execute(&client)
    .await?;
```

## Exemplo com conexão

Use `.with_connection(...)` quando você quer inserir um item e conectar uma relação já existente.

```rust
dinoco::insert_into::<User>()
    .values(new_user)
    .with_connection(existing_team)
    .execute(&client)
    .await?;
```

## Exemplo com retorno tipado

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: i64,
    name: String,
}

let created = dinoco::insert_into::<User>()
    .values(User { id: 1, name: "Matheus".to_string() })
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

## Próximos passos

- [**`insert_many::&lt;M&gt;()`**](/v0.0.1/orm/insert-many): inserção em lote.
- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): atualização de registros.
