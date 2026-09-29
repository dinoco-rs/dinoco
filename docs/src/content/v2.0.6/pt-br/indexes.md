# Índices e constraints

Índices no Dinoco são declarados direto no `schema.dinoco`, junto dos fields que cobrem, e passam pelo mesmo pipeline de migration, introspecção e detecção de drift que as tabelas — não existe uma etapa separada de "gerenciamento de índices". Esta página cobre três mecanismos distintos: índices comuns explícitos, índices que o Dinoco cria automaticamente, e busca full-text.

## Escolha o tipo de índice

| Necessidade | Declaração | Resultado |
| --- | --- | --- |
| Acelerar igualdade, ordenação ou range | `@index` | Índice B-tree não único |
| Acelerar um grupo ordenado de fields | `@@indexes([...])` | Índice composto não único |
| Impedir duplicatas em um grupo | `@@uniques([...])` | Índice unique composto |
| Pesquisar tokens em texto | `@fulltext` | GIN no PostgreSQL, `FULLTEXT` no MySQL e fallback no SQLite |
| Pesquisar um documento formado por vários fields | `@@fulltexts([...])` | Full-text composto nativo ou fallback SQLite |
| Primary key | `@id` ou `@@ids` | Índice automático da constraint |
| Foreign key | `@relation(fields: [...])` | Índice automático nas colunas locais |

> [!NOTE]
> Indexação comum e full-text são duas estratégias diferentes, não um espectro — um field não pode participar das duas. `@index`/`@@indexes` e `@fulltext`/`@@fulltexts` são mutuamente exclusivos no mesmo field.

## Declare um índice explícito

Adicione `@index` diretamente a um field escalar ou enum:

```dinoco
model Post {
    id           String   @id @default(uuid())
    slug         String   @index
    published_at DateTime @index(map: "idx_post_publication")
}
```

Sem `map`, o nome físico do índice segue `idx_<tabela>_<field>` automaticamente. Passe `map` quando precisar de um nome específico — por exemplo, para casar com um índice já existente numa migration vinda de outra ferramenta. De qualquer forma, `@index` nunca implica unicidade por si só; use `@unique` quando valores duplicados devem ser rejeitados.

## Declare índices e unicidade compostos

Declarações compostas são atributos de model, colocados no final do corpo do model:

```dinoco
model Article {
    tenant_id String
    id        String
    slug      String
    category  String
    title     String
    body      String?

    @@ids([tenant_id, id])
    @@uniques([tenant_id, slug])
    @@indexes([tenant_id, category])
    @@fulltexts([title, body])
}
```

`@@uniques` rejeita tuplas duplicadas no nível do banco e produz um índice unique composto; `@@indexes` produz o mesmo formato sem a constraint de unicidade. Os dois preservam fielmente a ordem dos fields durante migration, introspecção e comparação de drift — ordem importa para a utilidade de um índice composto, então o Dinoco nunca a reordena silenciosamente. Um model pode declarar vários atributos `@@uniques`/`@@indexes`/`@@fulltexts` quando precisa de mais de um grupo distinto.

> [!TIP]
> Todo array aqui precisa ser não vazio, referenciar fields que realmente existem no model, e não repetir nenhum field — e o formatter sempre move declarações `@@...` para depois dos fields, então não se preocupe em qual lugar do corpo do model você as escreve.

## Primary keys são indexadas

Todo field `@id` já é indexado, pela própria constraint `PRIMARY KEY` — o Dinoco rastreia esse índice no seu modelo interno de comparação (para que a detecção de drift saiba que ele já está contabilizado), mas nunca emite um segundo `CREATE INDEX` redundante para ele.

Uma primary key composta mantém a ordem declarada de fields nesse mesmo índice:

```dinoco
model Membership {
    tenant_id String
    user_id   String

    @@ids([tenant_id, user_id])
}
```

O banco acaba com um único índice composto sobre `(tenant_id, user_id)`, nessa ordem — útil para queries que filtram só por `tenant_id`, menos para queries que filtram só por `user_id`.

## Foreign keys são indexadas

Toda relação materializada ganha um índice automático sobre seus `fields`, sem precisar de `@index`:

```dinoco
model Session {
    id         String   @id @default(uuid())
    account_id String
    account    Account? @relation(
        fields: [account_id],
        references: [id]
    )
}
```

Isso gera `idx_session_account_id`. Uma relação composta (múltiplos `fields`) ganha um único índice composto, na mesma ordem em que a relação os declara.

Tabelas pivô de many-to-many implícito ganham três índices automaticamente:

1. O índice da primary key composta.
2. Um índice na foreign key do primeiro lado.
3. Um índice na foreign key do segundo lado.

> [!WARNING]
> Não adicione `@index` a um field só porque ele já faz parte de `@id` ou `@relation` — isso pede ao Dinoco para criar um segundo índice redundante ao lado do automático, gastando capacidade de escrita e espaço em disco sem benefício nenhum para as queries.

## Índices full-text

`@fulltext` só funciona em `String` ou `String?`:

```dinoco
model Article {
    id      String  @id @default(uuid())
    title   String  @fulltext
    summary String? @fulltext
}
```

Um model pode ter vários fields `@fulltext` **independentes**, cada um pesquisável por conta própria. `@@fulltexts([title, summary])` é uma coisa bem diferente: ele constrói *um* documento ordenado a partir dos dois fields e o apoia em *um* índice nativo, então chamar `.fulltext(...)` em qualquer um dos fields gerados pesquisa o grupo combinado inteiro. No SQLite — que não tem índice full-text nativo — o Dinoco pula a criação de um B-tree que não ajudaria em nada, e em vez disso pesquisa cada field do grupo com `LIKE '%termo%'`, unido por `OR`.

Veja [Busca full-text](/pt-br/docs/orm/orm/full-text-search) para como `.fulltext(...)` aparece em cada find builder.

## Fluxo de migration

Igual a qualquer outra mudança de schema — depois de editar um índice:

```bash
dinoco migrate generate
dinoco migrate run
```

O planner emite `CREATE INDEX`, `DROP INDEX`, ou a variante full-text específica do adapter ativo, conforme necessário. A introspecção compara nome, colunas, ordem e tipo de cada índice contra o que o schema descreve atualmente, então um índice alterado fora do Dinoco ainda é detectado como drift.

## Regras de validação

- `@index` aceita somente o argumento opcional `map: "nome"`.
- `@index` só funciona em fields escalares e enums — nunca em um field de relação.
- `@fulltext` não aceita argumento nenhum.
- `@fulltext` só funciona em `String` e `String?`.
- Um field nunca pode pertencer ao mesmo tempo a uma declaração comum e uma full-text, inclusive nas formas compostas.
- Vários fields `@fulltext` são permitidos no mesmo model.
- Todo membro de `@@fulltexts` precisa ser `String` ou `String?`, e pode pertencer a no máximo uma declaração full-text.
- `@@uniques`, `@@indexes` e `@@fulltexts` podem, cada um, ser repetidos para grupos de fields diferentes e independentes.
