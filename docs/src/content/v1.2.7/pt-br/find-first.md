# Find first

`find_first` busca no máximo uma row e retorna `Option<M>`. Use-o quando não encontrar um registro for um resultado esperado.

## Consulta básica

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .execute(&client)
    .await?;
```

O tipo é `anyhow::Result<Option<Account>>`. Converta `None` em erro somente na camada em que a ausência é inválida:

```rust
let account = account
    .ok_or_else(|| anyhow::anyhow!("account not found"))?;
```

## Filtre o resultado

Vários `where_` usam `AND`:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .where_(|account| account.email.ends_with("@example.com"))
    .execute(&client)
    .await?;
```

Para grupos explícitos, use [Where complex](/v1.2.7/orm/where-complex). Fields `@fulltext` também funcionam:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

## Ordene antes de escolher

`find_first` aceita um `order_by`:

```rust
let newest = dinoco::find_first::<Account>()
    .where_(|account| account.active.eq(true))
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

Sem ordenação, o banco pode escolher qualquer row compatível.

## Selecione e inclua

`select::<S>()` muda o retorno para `Option<S>`. `includes(...)` carrega relações:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .includes(|account| account.sessions())
    .execute(&client)
    .await?;
```

Consulte [Select](/v1.2.7/orm/select) e [Includes](/v1.2.7/orm/includes).

## Leia no primary

Com réplicas configuradas, use `read_in_primary()` quando a consulta precisa enxergar um write recente:

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.id.eq(&account_id))
    .read_in_primary()
    .execute(&client)
    .await?;
```

Todos os includes dessa consulta seguem o primary. A API transacional baseada em closure aceita builders de mutation; faça reads comuns com `find_first` pelo client antes ou depois da closure.
