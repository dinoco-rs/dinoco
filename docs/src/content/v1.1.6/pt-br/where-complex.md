# Where complex

`where_complex` cria uma árvore booleana com parênteses explícitos. Use-o quando uma sequência de `where_` unidos por `AND` não representa a regra da consulta.

## Entenda x e m

```rust
.where_complex(|x, m| {
    // x: AccountWhere
    // m: manipulador de grupos
})
```

`x` é o `EntityWhere` gerado para o model. Ele expõe os fields e seus operadores tipados.

`m` monta a estrutura lógica:

| Method | Uso |
| --- | --- |
| `m.and([a, b, ...])` | Todas as condições |
| `m.or(a, b)` | Uma das duas condições |
| `m.or_many([a, b, ...])` | Uma das várias condições |
| `m.not(a)` | Negação de uma condição ou grupo |

## Monte grupos aninhados

```rust
let account = dinoco::find_first::<Account>()
    .where_complex(|x, m| {
        m.or(
            m.and([
                x.id.eq("id"),
                x.name.eq("matheus"),
            ]),
            m.or(
                m.and([
                    x.id.eq("second-id"),
                    x.name.eq("ana"),
                ]),
                m.and([
                    x.id.eq("third-id"),
                    m.not(x.name.eq("blocked")),
                ]),
            ),
        )
    })
    .execute(&client)
    .await?;
```

Cada grupo mantém seus parênteses no SQL. Os valores continuam parâmetros do adapter e seguem a ordem da árvore.

## Where complex substitui where

Ao usar `where_complex`, todo `where_` do mesmo builder é ignorado, independentemente da ordem:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|x| x.active.eq(false)) // ignorado
    .where_complex(|x, m| {
        m.or(x.role.eq(Role::ADMIN), x.active.eq(true))
    })
    .where_(|x| x.deleted_at.not_null()) // ignorado
    .execute(&client)
    .await?;
```

Uma segunda chamada de `where_complex` substitui a árvore anterior.

## Builders suportados

`where_complex` funciona em:

- `find_first`;
- `find_many`;
- `find_and_update`;
- relações `one` e `many` dentro de `includes`;
- `find_first` e `find_many` adicionados a uma `Transaction`.

O mesmo comportamento de ignorar `where_` vale nos builders de relação.

## Combine com full-text

Fields `@fulltext` podem participar de qualquer grupo:

```rust
let articles = dinoco::find_many::<Article>()
    .where_complex(|article, m| {
        m.and([
            article.body.fulltext("dinoco"),
            m.not(article.body.fulltext("deprecated")),
        ])
    })
    .execute(&client)
    .await?;
```

## Grupos vazios

Evite montar `and([])` ou `or_many([])` a partir de input vazio. Construa a lista de condições na aplicação e escolha explicitamente entre não filtrar, retornar vazio ou executar um grupo não vazio.
