# Busca full-text

O Dinoco oferece busca full-text nativa no PostgreSQL e MySQL, com um fallback automático baseado em substring no SQLite, para que o mesmo código de aplicação funcione em todo lugar. O method `.fulltext(...)` só existe em fields que explicitamente optam pelo recurso no `schema.dinoco` — não tem como chamá-lo por acidente num field que não está indexado para isso.

## 1. Marque os fields pesquisáveis

```dinoco
model Account {
    id        String  @id @default(uuid())
    name      String  @fulltext
    biography String? @fulltext
    email     String
    reviewed  Boolean @default(false)
}
```

Um model pode ter vários fields `@fulltext` independentes; cada um ganha seu próprio índice dedicado no PostgreSQL e MySQL. As regras são restritas e verificadas em tempo de compilação:

- Somente `String` e `String?`.
- Nenhum argumento.
- Não pode coexistir com `@index` no mesmo field.
- Não pode ser aplicado a um field de relação.

### Forme um documento com vários fields

Use `@@fulltexts` quando uma única busca deve cobrir um grupo ordenado de fields juntos, como um documento combinado:

```dinoco
model Article {
    id       String  @id @default(uuid())
    title    String
    subtitle String?
    body     String

    @@fulltexts([title, subtitle, body])
}
```

Todo membro do grupo ganha a capability gerada `.fulltext(...)`, e chamá-la a partir de *qualquer um* deles — `article.title.fulltext("dinoco")`, `article.subtitle.fulltext(...)` ou `article.body.fulltext(...)` — pesquisa exatamente o mesmo documento combinado, não só aquele field. Isso importa concretamente no MySQL: a query gerada é um único `MATCH(title, subtitle, body)`, batendo coluna por coluna com o índice `FULLTEXT` composto.

> [!TIP]
> Use declarações `@fulltext` separadas por field quando eles realmente precisarem de índices e buscas *independentes*. Um field só pode pertencer a uma declaração full-text — nunca um `@fulltext` solo e membro de um grupo `@@fulltexts` ao mesmo tempo — e ele também não pode se sobrepor a `@index`/`@@indexes` de nenhuma forma.

## 2. Gere a migration e os models

```bash
dinoco migrate generate
dinoco migrate run
```

Junto do índice nativo (onde o adapter suporta um), o model Rust gerado ganha exatamente a capability necessária para chamar `.fulltext(...)`:

```rust
account.name.fulltext("matheus");
```

`account.email.fulltext(...)` simplesmente não compila — `email` não tem `@fulltext` nem participa de nenhum grupo `@@fulltexts`, então o method não existe nele de forma alguma.

## Use em find first

```rust
let account = dinoco::find_first::<Account>()
    .where_(|account| account.name.fulltext("matheus"))
    .execute(&client)
    .await?;
```

O tipo de retorno não é afetado — continua `Option<Account>`.

## Use em find many

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.biography.fulltext("rust database"))
    .order_by(|account| account.id.asc())
    .execute(&client)
    .await?;
```

Continua `Vec<Account>` — full-text é só mais uma condição, combinável com tudo que o `find_many` já suporta.

## Use em find and update

```rust
let account = dinoco::find_and_update::<Account>()
    .where_(|account| account.biography.fulltext("dinoco"))
    .update(|account| account.reviewed.set(true))
    .execute(&client)
    .await?;
```

A condição full-text aqui seleciona qual row é atualizada e retornada — veja [Find and update](/pt-br/docs/orm/orm/find-and-update) para o resto do comportamento desse builder.

## Use em relation includes

O mesmo method funciona dentro de builders de include "um" e "muitos" gerados:

```rust
let accounts = dinoco::find_many::<Account>()
    .includes(|account| {
        account
            .sessions()
            .where_(|session| session.label.fulltext("mobile"))
    })
    .execute(&client)
    .await?;
```

O field da entity relacionada precisa ter `@fulltext` por conta própria — isso não é herdado da configuração do model pai.

## Combine com where complex

```rust
let accounts = dinoco::find_many::<Account>()
    .where_complex(|account, m| {
        m.and([
            account.biography.fulltext("dinoco"),
            m.not(account.biography.fulltext("deprecated")),
        ])
    })
    .execute(&client)
    .await?;
```

Uma chamada `.fulltext(...)` é um valor de condição normal do ponto de vista do `where_complex` — pode aparecer dentro de qualquer grupo `and`, `or`, `or_many` ou `not`, aninhado à vontade.

## Use em transactions

Condições full-text passam intactas para uma mutation executada pelo contexto transacional:

```rust
let account = dinoco::transaction(&client, |tx| async move {
    let account = dinoco::find_and_update::<Account>()
        .where_(|account| account.name.fulltext("matheus"))
        .update(|account| account.reviewed.set(true))
        .execute(tx)
        .await?;

    Ok(account)
})
.await?;
```

O adapter compila exatamente o mesmo predicado full-text no `UPDATE` que executa na conexão da transaction — nada no full-text se comporta diferente dentro de uma transaction.

## Comportamento por banco

| Adapter | Query | Índice |
| --- | --- | --- |
| PostgreSQL / PgBouncer | `to_tsvector('simple', ...) @@ plainto_tsquery('simple', ...)` | GIN de expressão |
| MySQL | `MATCH (...) AGAINST (... IN NATURAL LANGUAGE MODE)` | `FULLTEXT` |
| SQLite | `(field_a LIKE '%termo%' OR field_b LIKE '%termo%')` | Nenhum índice nativo |

Para uma declaração composta `@@fulltexts`, o PostgreSQL constrói um documento de texto concatenado, usado de forma idêntica pelo índice GIN e por toda query contra ele. O MySQL mantém a lista exata de colunas declarada na cláusula `MATCH(...)`. O SQLite, não tendo mecanismo full-text nativo, pesquisa uma substring simples em todo field membro — o que pode acabar varrendo a tabela inteira num dataset grande.

## Comportamento das migrations

PostgreSQL e MySQL nomeiam seus índices gerados como `idx_<tabela>_<fields...>_fulltext`. O planner de migration preserva a ordem declarada dos fields e trata criação, remoção, introspecção e detecção de drift desses índices separadamente dos índices comuns/unique.

> [!NOTE]
> O SQLite pula completamente a geração de migration de índice para fields full-text — um índice B-tree comum não acelera em nada uma query `LIKE '%termo%'` com wildcard no início, então o Dinoco não cria um que só ocuparia espaço e overhead de escrita à toa.

## Limitações

- `@fulltext` cria um índice independente de um único field; `@@fulltexts([...])` cria um documento combinado de múltiplos fields em vez disso.
- Um field pertence a no máximo uma declaração full-text, e nunca se sobrepõe a `@index`/`@@indexes`.
- Todo membro de um grupo composto precisa ser `String` ou `String?`.
- Ranking de relevância, idioma/stemming configurável, busca por frase e tokenizers customizados estão todos fora do que a busca full-text do Dinoco faz hoje — esse é intencionalmente o subconjunto comum entre três implementações de full-text de bancos bem diferentes, não um wrapper em cima do conjunto completo de recursos de cada um.
- O fallback baseado em substring do SQLite tem semântica de match diferente da busca nativa por tokens do PostgreSQL/MySQL — espere um comportamento estilo `LIKE` ali, não stemming ou consciência de limite de palavra.
