# Where complex

`where_complex` monta uma árvore de condições booleanas com agrupamento explícito, com parênteses no SQL. Use no momento em que uma sequência simples de `where_` unidos por `AND` não consegue mais expressar a regra que você precisa — qualquer coisa envolvendo `OR`, `NOT`, ou precedência mista.

## Entenda x e m

```rust
.where_complex(|x, m| {
    // x: AccountWhere
    // m: manipulador de grupos
})
```

`x` é o mesmo `EntityWhere` gerado que você já conhece do `where_` — os mesmos fields, os mesmos operadores tipados, nada novo para aprender ali.

`m` é a novidade: ele monta a estrutura lógica em volta dessas condições.

| Method | Uso |
| --- | --- |
| `m.and([a, b, ...])` | Toda condição precisa valer |
| `m.or(a, b)` | Uma das duas condições vale |
| `m.or_many([a, b, ...])` | Qualquer uma entre várias condições vale |
| `m.not(a)` | Nega uma condição ou um grupo inteiro |

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

Cada grupo `m.and`/`m.or` vira seu próprio grupo com parênteses no SQL gerado, aninhado exatamente na mesma profundidade em que você aninha no Rust. Os valores continuam viajando como parâmetros do adapter, na mesma ordem em que a árvore é construída.

## Where complex substitui where

No momento em que um builder usa `where_complex`, todo `where_` nesse mesmo builder é ignorado — independentemente de você ter escrito antes ou depois da chamada de `where_complex`:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|x| x.active.eq(false)) // ignorado
    .where_complex(|x, m| {
        m.or(x.role.eq(Role::Admin), x.active.eq(true))
    })
    .where_(|x| x.deleted_at.not_null()) // ignorado
    .execute(&client)
    .await?;
```

> [!WARNING]
> Essa sobrescrita silenciosa é fácil de passar batido num diff — adicionar um `where_complex` a um builder que já tinha `where_` não gera erro de compilação nem warning em runtime, só descarta os filtros comuns silenciosamente. Procure por chamadas `where_` existentes antes de adicionar `where_complex` a um builder que já pode ter alguma. Chamar `where_complex` uma segunda vez no mesmo builder substitui a árvore anterior da mesma forma, sem mesclar com ela.

## Builders suportados

`where_complex` funciona em:

- `find_first`
- `find_many`
- `find_and_update`
- builders de relação `one`/`many` dentro de `includes`

Quando `find_and_update` executa com `tx` dentro de uma transaction, a mesma árvore de condição complexa é compilada direto no `UPDATE` transacional — não precisa de nenhum tratamento especial para isso funcionar ali também. Builders de relação seguem exatamente a mesma regra de "where_complex vence silenciosamente" que os finds de nível superior.

## Combine com full-text

Fields `@fulltext` participam de um grupo complexo exatamente como qualquer outra condição:

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

> [!WARNING]
> Não chame `and([])` ou `or_many([])` com uma lista construída a partir de input vazio em runtime — um grupo `AND` vazio e um grupo `OR` vazio têm significados SQL opostos e fáceis de trocar ("bate com tudo" vs. "não bate com nada"), e nenhum dos dois provavelmente é o que você queria. Quando as condições vêm de input dinâmico da aplicação, construa a lista explicitamente e decida de antemão o que uma lista vazia deve significar: pular o filtro por completo, retornar vazio, ou cair para um grupo específico não vazio.
