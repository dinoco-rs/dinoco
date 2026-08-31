# Find many

`find_many` retorna todas as rows compatíveis como `Vec<M>`. Sem correspondências, o retorno é um vetor vazio.

## Consulta básica

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true))
    .execute(&client)
    .await?;
```

Vários `where_` são combinados com `AND`. Use [Where complex](/v1.2.7/orm/where-complex) quando a precedência precisar de grupos `AND`, `OR` e `NOT`.

## Ordene os resultados

```rust
let accounts = dinoco::find_many::<Account>()
    .order_by(|account| account.created_at.desc())
    .execute(&client)
    .await?;
```

O builder aceita uma ordenação tipada com `asc()` ou `desc()`.

## Paginação

`take` limita a quantidade e `skip` define o offset:

```rust
let page = dinoco::find_many::<Account>()
    .order_by(|account| account.id.asc())
    .take(25)
    .skip(50)
    .execute(&client)
    .await?;
```

Sempre combine paginação por offset com uma ordenação estável.

## Busca full-text

Fields marcados com `@fulltext` expõem o method:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .execute(&client)
    .await?;
```

Consulte [Busca full-text](/v1.2.7/orm/full-text-search) para as diferenças entre adapters.

## Selecione e inclua

```rust
let accounts = dinoco::find_many::<Account>()
    .select::<AccountSummary>()
    .execute(&client)
    .await?;
```

O retorno passa a ser `Vec<AccountSummary>`. `includes(...)` pode carregar relações, filtrar children e aplicar paginação por parent. Veja [Select](/v1.2.7/orm/select) e [Includes](/v1.2.7/orm/includes).

## Leia no primary

`read_in_primary()` ignora réplicas nessa consulta e em todos os includes. Use-o em leituras dependentes de um write recente. A API transacional baseada em closure aceita builders de mutation; faça reads comuns com `find_many` pelo client antes ou depois da closure.
