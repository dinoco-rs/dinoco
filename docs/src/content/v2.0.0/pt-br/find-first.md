# Find first

`find_first` busca no máximo uma row e retorna `Option<M>`. Use-o sempre que um registro ausente for um resultado normal e esperado — não uma falha.

## Consulta básica

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .execute(&client)
    .await?;
```

O tipo completo aqui é `anyhow::Result<Option<Account>>` — duas camadas de "pode não dar certo", e elas significam coisas diferentes. Só converta o `None` em erro no ponto onde a ausência de fato *é* inválida para o seu caso de uso:

```rust
let account = account.ok_or_else(|| anyhow::anyhow!("account not found"))?;
```

## Filtre o resultado

Várias chamadas `where_` se combinam com `AND`:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .where_(|account| account.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Precisa de `OR`/`NOT` em vez disso? Veja [Where complex](/pt-br/docs/orm/orm/where-complex). Fields `@fulltext` também funcionam aqui, pelo mesmo `where_`:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

## Ordene antes de escolher

`find_first` aceita exatamente um `order_by`:

```rust
let newest = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

> [!WARNING]
> Sem um `order_by`, qual das várias rows compatíveis volta fica a critério do banco — ele pode retornar qualquer uma delas, e essa escolha pode até mudar entre duas execuções da mesma query, sem nenhuma outra mudança. Se "a primeira row compatível" precisa significar algo específico (a mais nova, a de maior prioridade), diga isso explicitamente com `order_by`.

## Selecione e inclua

`select::<S>()` muda o tipo de retorno para `Option<S>`; `includes(...)` carrega uma relação junto da row:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .includes(|account| account.sessions())
    .execute(&client)
    .await?;
```

Veja [Select](/pt-br/docs/orm/orm/select) e [Includes](/pt-br/docs/orm/orm/includes) para os detalhes de cada um.

## Leia no primary

Com réplicas configuradas, adicione `read_in_primary()` quando essa query específica precisa observar um write que acabou de acontecer:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .read_in_primary()
    .execute(&client)
    .await?;
```

Todo `.includes(...)` dessa mesma query segue ela para a primary também — não dá para rotear a row principal para a primary enquanto deixa seus includes caírem numa réplica. Note que a API transacional por closure só aceita builders de mutation, não `find_first`; faça leituras comuns como essa direto pelo `&client`, antes ou depois da closure de transaction.
