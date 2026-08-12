# Índices e constraints

Índices no Dinoco são declarados no `schema.dinoco` e passam pelo mesmo fluxo de migration, introspecção e detecção de drift das tabelas. Esta página separa os três casos: índices explícitos, índices automáticos e busca full-text.

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

Declarações comuns e full-text representam estratégias diferentes. Um field não pode participar ao mesmo tempo de `@index`/`@@indexes` e `@fulltext`/`@@fulltexts`.

## Declare um índice explícito

Adicione `@index` a um field escalar ou enum:

```dinoco
model Post {
    id           String   @id @default(uuid())
    slug         String   @index
    published_at DateTime @index(map: "idx_post_publication")
}
```

Sem `map`, o nome segue `idx_<tabela>_<field>`. `map` permite definir o nome físico. O índice não adiciona unicidade; use `@unique` quando duplicatas não forem válidas.

## Declare índices e unicidade compostos

Coloque os grupos ordenados no final do model:

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

`@@uniques` rejeita tuples duplicadas e gera um índice unique composto. `@@indexes` é não único. Os dois preservam a ordem dos fields em migration, introspecção e comparação de drift. As declarações podem ser repetidas quando o model precisa de grupos diferentes.

O formatter move toda declaração `@@...` para depois dos fields. Cada array deve ser não vazio, referenciar scalars ou enums existentes e não repetir nenhum field.

## Primary keys são indexadas

Todo field `@id` é indexado pela própria constraint `PRIMARY KEY`. O Dinoco representa esse índice no schema desejado para comparação, mas não gera um `CREATE INDEX` duplicado.

Em uma primary key composta, a ordem declarada é preservada:

```dinoco
model Membership {
    tenant_id String
    user_id   String

    @@ids([tenant_id, user_id])
}
```

O banco mantém um índice composto em `(tenant_id, user_id)`.

## Foreign keys são indexadas

Toda relação materializada recebe um índice automático sobre `fields`:

```dinoco
model Session {
    id         String  @id @default(uuid())
    account_id String
    account    Account @relation(
        fields: [account_id],
        references: [id]
    )
}
```

O índice gerado é `idx_session_account_id`. Relações compostas recebem um único índice composto, na mesma ordem de `fields`.

Tabelas pivô many-to-many implícitas recebem:

1. o índice da primary key composta;
2. um índice para a foreign key do primeiro lado;
3. um índice para a foreign key do segundo lado.

Não repita `@index` apenas porque o field já participa de `@id` ou `@relation`.

## Índices full-text

Use `@fulltext` somente em `String` ou `String?`:

```dinoco
model Article {
    id      String  @id @default(uuid())
    title   String  @fulltext
    summary String? @fulltext
}
```

Um model pode ter vários fields `@fulltext` independentes. Já `@@fulltexts([title, summary])` cria um único documento ordenado e um único índice nativo. Chamar `.fulltext(...)` em qualquer field gerado pesquisa o grupo completo. O SQLite não cria um B-tree ineficaz e pesquisa cada field do grupo com `LIKE '%termo%'`, unido por `OR`.

Consulte [Busca full-text](/v1.1.6/orm/full-text-search) para a API `.fulltext(...)` em todos os finds.

## Fluxo de migration

Depois de alterar um índice:

```bash
dinoco migrate generate
dinoco migrate run
```

O planner gera `CREATE INDEX`, `DROP INDEX` ou a variante full-text do adapter. A introspecção compara nome, colunas, ordem e tipo do índice. Snapshots criados antes da v1.1.6 continuam compatíveis: a ausência histórica da propriedade `indexes` não é tratada como remoção intencional.

## Regras de validação

- `@index` aceita somente `map: "nome"`.
- `@index` funciona em fields escalares e enums, não em fields de relação.
- `@fulltext` não aceita argumentos.
- `@fulltext` funciona somente em `String` e `String?`.
- Um field não pode pertencer ao mesmo tempo a uma declaração comum e uma full-text, inclusive nas formas compostas.
- Vários `@fulltext` são permitidos no mesmo model.
- Todo membro de `@@fulltexts` deve ser `String` ou `String?` e pode pertencer a apenas uma declaração full-text.
- `@@uniques`, `@@indexes` e `@@fulltexts` podem ser repetidos para grupos ordenados diferentes.
