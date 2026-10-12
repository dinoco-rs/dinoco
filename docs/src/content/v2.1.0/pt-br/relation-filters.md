# Filtros de relação

Todo field de relação de um model também existe no tipo `Where` gerado. Filtrar por ele restringe as rows consultadas pelas rows relacionadas — "transactions pagas via Pix", "pedidos sem nenhum item estornado", "usuários cujo gestor é a Ana" — sem carregar a relação e sem escrever um join.

## Filtre por um registro relacionado

```dinoco
enum PayoutMethod {
    PIX
    TED
    BOLETO
}

model Bank {
    id   String @id @default(uuid())
    name String
}

model Payout {
    id           String        @id @default(uuid())
    method       PayoutMethod
    bank_id      String?
    bank         Bank?         @relation(fields: [bank_id], references: [id])
    transactions Transaction[] @relation(fields: [id], references: [payout_id])
}

model Transaction {
    id        String  @id @default(uuid())
    amount    Integer
    payout_id String?
    payout    Payout? @relation(fields: [payout_id], references: [id])
    items     Item[]  @relation(fields: [id], references: [transaction_id])
}

model Item {
    id             String       @id @default(uuid())
    transaction_id String
    refunded       Boolean      @default(false)
    note           String?
    transaction    Transaction? @relation(fields: [transaction_id], references: [id])
}
```

`transaction.payout` recebe um callback com `PayoutWhere` — os mesmos fields tipados que você usa em `find_many::<Payout>()`:

```rust
let transactions = dinoco::find_many::<Transaction>()
    .where_(|transaction| transaction.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
    .execute(&client)
    .await?;
```

Só voltam as transactions cujo payout bate com o filtro, e `transaction.payout` continua `None` em todas — um filtro de relação decide *quais* rows voltam, ele nunca carrega nada. Adicione `.includes(|x| x.payout())` quando também quiser o próprio payout.

O filtro vira um único statement:

```sql
SELECT id, amount, payout_id FROM transaction
WHERE EXISTS (
    SELECT 1 FROM payout AS __dinoco_relation_1
    WHERE __dinoco_relation_1.id = transaction.payout_id
      AND __dinoco_relation_1.method = ?
)
```

Filtros de relação se combinam com qualquer outra condição via `AND`, como todo `where_`:

```rust
let transaction = dinoco::find_first::<Transaction>()
    .where_(|transaction| transaction.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
    .where_(|transaction| transaction.amount.gt(200))
    .execute(&client)
    .await?;
```

## Methods de filtro

| Method | Mantém as rows que... | SQL |
| --- | --- | --- |
| `where_(...)` | têm pelo menos uma row relacionada que bate com a condição | `EXISTS (...)` |
| `where_complex(...)` | o mesmo, com uma árvore de [where complex](/pt-br/docs/orm/orm/where-complex) sobre a row relacionada | `EXISTS (...)` |
| `none(...)` | não têm nenhuma row relacionada que bate com a condição — inclusive rows sem relacionadas | `NOT EXISTS (...)` |
| `every(...)` | só têm rows relacionadas que batem com a condição — inclusive rows sem relacionadas | `NOT EXISTS (... AND (condição) IS NOT TRUE)` |
| `exists()` | têm pelo menos uma row relacionada | `EXISTS (...)` |
| `not_exists()` | não têm nenhuma row relacionada | `NOT EXISTS (...)` |

```rust
// Transactions com algum item estornado.
dinoco::find_many::<Transaction>().where_(|t| t.items.where_(|item| item.refunded.eq(true)));

// Transactions sem nenhum item estornado, inclusive as que não têm itens.
dinoco::find_many::<Transaction>().where_(|t| t.items.none(|item| item.refunded.eq(true)));

// Transactions com todos os itens estornados, inclusive as que não têm itens.
dinoco::find_many::<Transaction>().where_(|t| t.items.every(|item| item.refunded.eq(true)));

// Transactions com pelo menos um item / sem payout.
dinoco::find_many::<Transaction>().where_(|t| t.items.exists());
dinoco::find_many::<Transaction>().where_(|t| t.payout.not_exists());
```

> [!NOTE]
> `every` e `none` são verdadeiros para uma row sem nenhuma relacionada. Adicione `.exists()` quando "todos os itens foram estornados" também precisar de pelo menos um item: `.where_(|t| t.items.every(...)).where_(|t| t.items.exists())`.

`every` só conta uma row relacionada como compatível quando a condição dela é `TRUE`. Uma comparação contra uma coluna `NULL` não é nem verdadeira nem falsa em SQL, então um item sem `note` **não** satisfaz `every(|item| item.note.eq("gift"))` — a mesma row também não voltaria em `find_many::<Item>().where_(|item| item.note.eq("gift"))`.

## Todos os formatos de relação

Os mesmos methods existem em todo field de relação; o que "rows relacionadas" significa segue a relação:

| Relação | Field | Rows relacionadas |
| --- | --- | --- |
| Many-to-one | `transaction.payout` | O payout para o qual `payout_id` aponta, se houver |
| One-to-many | `payout.transactions` | Toda transaction cujo `payout_id` é este payout |
| One-to-one, lado dono | `profile.user` | O user para o qual `user_id` aponta, se houver |
| One-to-one, outro lado | `user.profile` | O profile cujo `user_id` é este user, se houver |
| Many-to-many implícito | `business.systems` | Todo system ligado pela tabela pivot |
| Self relation | `employee.manager`, `employee.reports` | Rows do mesmo model, ligadas pelas chaves da relação |

```rust
// O outro lado de um one-to-one.
dinoco::find_many::<User>().where_(|user| user.profile.where_(|profile| profile.bio.like("rust")));

// Many-to-many implícito: filtre por qualquer field do outro lado, não só pelo id.
dinoco::find_many::<Business>().where_(|business| business.systems.where_(|system| system.name.eq("ERP")));

// Self relation: funcionários geridos pela Ana, e gestores sem subordinados.
dinoco::find_many::<Employee>().where_(|employee| employee.manager.where_(|manager| manager.name.eq("Ana")));
dinoco::find_many::<Employee>().where_(|employee| employee.reports.not_exists());
```

Numa relação singular, `where_` significa "a row relacionada existe e bate com a condição", e `none` significa "não há row relacionada, ou ela não bate" — então `transaction.payout.none(|p| p.method.eq(PayoutMethod::Pix))` também retorna as transactions sem payout.

## Mesma row ou rows diferentes

As condições dentro de **um** callback precisam valer para a **mesma** row relacionada. Filtros de relação separados são independentes, então cada um pode ser satisfeito por uma row relacionada diferente:

```rust
// Um único item estornado *e* com a nota "gift".
dinoco::find_many::<Transaction>()
    .where_(|t| t.items.where_complex(|item, m| m.and([item.refunded.eq(true), item.note.eq("gift")])));

// Algum item estornado, e algum item (talvez outro) com a nota "gift".
dinoco::find_many::<Transaction>()
    .where_(|t| t.items.where_(|item| item.refunded.eq(true)))
    .where_(|t| t.items.where_(|item| item.note.eq("gift")));
```

## Aninhe filtros de relação

O callback recebe o próprio `Where` do model relacionado, fields de relação inclusos, então os filtros se aninham até onde o schema for:

```rust
// Transactions pagas para uma conta do Nubank.
let transactions = dinoco::find_many::<Transaction>()
    .where_(|t| t.payout.where_(|payout| payout.bank.where_(|bank| bank.name.eq("Nubank"))))
    .execute(&client)
    .await?;

// Payouts com pelo menos uma transaction que tem item estornado.
let payouts = dinoco::find_many::<Payout>()
    .where_(|payout| payout.transactions.where_(|t| t.items.where_(|item| item.refunded.eq(true))))
    .execute(&client)
    .await?;
```

Cada nível vira uma subquery própria, correlacionada com o nível de cima, e tudo continua rodando como um único statement.

## Combine com where complex

Um filtro de relação é uma condição comum, então entra em qualquer ponto de uma árvore de [where complex](/pt-br/docs/orm/orm/where-complex):

```rust
let transactions = dinoco::find_many::<Transaction>()
    .where_complex(|t, m| {
        m.and([
            t.amount.gte(100),
            m.or(t.payout.where_(|payout| payout.method.eq(PayoutMethod::Ted)), t.items.not_exists()),
            m.not(t.items.where_(|item| item.refunded.eq(true))),
        ])
    })
    .execute(&client)
    .await?;
```

`m.not(t.payout.where_(...))` mantém as transactions sem payout, exatamente como `t.payout.none(...)`: a subquery é sempre verdadeira ou falsa, nunca `NULL`.

## Builders suportados

Um filtro de relação funciona em qualquer lugar que aceita `where_` (ou `where_complex`):

- `find_first`, `find_many` e as formas com `.select::<S>()`, `.pluck(...)` e `.transform(...)`;
- `count` e `exists`;
- `update`, `update_many`, `find_and_update`, `delete` e `delete_many`, inclusive com `.returning::<S>()`;
- o `where_` dos builders de relação dentro de `.includes(...)` e de `count(...).includes(...)`;
- tudo acima com `.execute(tx)` dentro de uma [transaction](/pt-br/docs/orm/orm/transactions).

```rust
let pix = dinoco::count::<Transaction>()
    .where_(|t| t.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
    .execute(&client)
    .await?
    .total;

dinoco::update_many::<Transaction>()
    .where_(|t| t.payout.where_(|payout| payout.method.eq(PayoutMethod::Boleto)))
    .update(|t| t.amount.set(0))
    .execute(&client)
    .await?;

dinoco::delete_many::<Item>()
    .where_(|item| item.transaction.where_(|t| t.payout.not_exists()))
    .execute(&client)
    .await?;
```

## Filtros de relação versus includes

Os dois recebem um `where_` sobre o model relacionado, mas respondem perguntas diferentes:

| | Filtro de relação | Filtro de include |
| --- | --- | --- |
| Como se escreve | `t.items.where_(...)` dentro de `.where_(...)` | `t.items().where_(...)` dentro de `.includes(...)` |
| Decide | quais rows **pai** voltam | quais rows **relacionadas** são carregadas em cada pai |
| Carrega a relação | não | sim |

Eles se combinam. Retorne só os payouts que têm uma transaction estornada, e carregue apenas essas transactions:

```rust
let payouts = dinoco::find_many::<Payout>()
    .where_(|payout| payout.transactions.where_(|t| t.items.where_(|item| item.refunded.eq(true))))
    .includes(|payout| payout.transactions().where_(|t| t.items.where_(|item| item.refunded.eq(true))))
    .execute(&client)
    .await?;
```

O `where_` do próprio include também aceita filtros de relação, como acima: ali, `t.items.where_(...)` filtra as transactions que estão sendo carregadas.

## SQL gerado e performance

- **Um statement, sem fan-out.** Um filtro de relação é uma subquery `EXISTS` correlacionada, não um join, então um pai com muitos filhos compatíveis continua voltando uma única vez — sem rows duplicadas e sem `DISTINCT`. O banco para de olhar as rows relacionadas de um pai assim que uma delas decide o resultado, e PostgreSQL e MySQL 8 planejam `EXISTS`/`NOT EXISTS` como semi-joins e anti-joins.
- **Lookups indexados.** A subquery é correlacionada pelas chaves da relação: a chave referenciada (normalmente a primary key) num many-to-one ou no lado dono de um one-to-one, e a foreign key na direção oposta. O Dinoco [indexa toda foreign key](/pt-br/docs/orm/guide/indexes#foreign-keys-sao-indexadas) e as duas colunas da pivot de um many-to-many implícito, então cada lookup é uma busca em índice.
- **Self relations e aninhamento.** Cada nível dá um alias às suas tabelas por profundidade (`__dinoco_relation_1`, `__dinoco_relation_2`, ...), então uma self relation nunca confunde a row filtrada com a row relacionada.
- **Valores com bind.** Os valores dentro de filtros de relação são parâmetros com bind, na ordem em que aparecem, como em qualquer outro filtro.
- **Escritas no MySQL.** O MySQL recusa um `UPDATE`/`DELETE` que lê a própria tabela escrita numa subquery (erro 1093) — por exemplo `update_many::<Employee>()` filtrado por `employee.manager`. O Dinoco compila exatamente esses filtros como uma derived table das chaves compatíveis, que o MySQL materializa antes de escrever, então funcionam sem mudança nenhuma. Leituras, escritas que não leem a própria tabela e os outros bancos continuam com a subquery correlacionada simples.
