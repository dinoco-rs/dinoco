# Buscar registros

Builders de find são lazy: encadear methods apenas descreve a consulta. `.execute(&client).await` compila o SQL no adapter e faz o I/O.

## Busque o primeiro registro

```rust
let user = dinoco::find_first::<User>()
    .where_(|x| x.email.eq("ana@example.com"))
    .execute(&client)
    .await?;
```

O retorno é `Option<User>`. Converta para erro somente quando a ausência for realmente excepcional:

```rust
let user = user.ok_or_else(|| anyhow::anyhow!("user not found"))?;
```

## Busque vários registros

```rust
let users = dinoco::find_many::<User>()
    .where_(|x| x.active.eq(true))
    .order_by(|x| x.created_at.desc())
    .execute(&client)
    .await?;
```

O retorno é `Vec<User>`; sem correspondências, o vetor vem vazio.

## Ordenação e paginação

Os dois finds aceitam `.order_by(|x| x.field.asc())` ou `.desc()`. `find_many` também possui `.take()` e `.skip()`:

```rust
let page = dinoco::find_many::<User>()
    .order_by(|x| x.id.asc())
    .take(25)
    .skip(50)
    .execute(&client)
    .await?;
```

Use uma ordenação estável com paginação; sem ela, o banco não garante sequência entre requests.

## Leia no primary

```rust
let user = dinoco::find_first::<User>()
    .where_(|x| x.id.eq(&user_id))
    .read_in_primary()
    .execute(&client)
    .await?;
```

Use isso quando a leitura precisa enxergar um write recente. O sinal também acompanha todos os includes.

## Valores retornados

| Builder | Retorno |
| --- | --- |
| `find_first::<M>()` | `Result<Option<M>>` |
| `find_many::<M>()` | `Result<Vec<M>>` |
| `find_first::<M>().select::<S>()` | `Result<Option<S>>` |
| `find_many::<M>().select::<S>()` | `Result<Vec<S>>` |

Falhas de banco, decode ou include são erros. Não encontrar rows não é.
