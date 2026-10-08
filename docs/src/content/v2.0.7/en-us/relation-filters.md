# Relation filters

Every relation field of a model also exists on its generated `Where` type. Filtering through it narrows the rows you query by their related rows — "transactions paid out through Pix", "orders with no refunded item", "users whose manager is Ana" — without loading the relation and without writing a join.

## Filter by a related record

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

`transaction.payout` takes a callback that receives `PayoutWhere` — the same typed fields you use in `find_many::<Payout>()`:

```rust
let transactions = dinoco::find_many::<Transaction>()
    .where_(|transaction| transaction.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
    .execute(&client)
    .await?;
```

Only transactions whose payout matches come back, and `transaction.payout` is still `None` on every one of them — a relation filter decides *which* rows are returned, it never loads anything. Add `.includes(|x| x.payout())` when you also want the payout itself.

The filter compiles into a single statement:

```sql
SELECT id, amount, payout_id FROM transaction
WHERE EXISTS (
    SELECT 1 FROM payout AS __dinoco_relation_1
    WHERE __dinoco_relation_1.id = transaction.payout_id
      AND __dinoco_relation_1.method = ?
)
```

Relation filters combine with every other condition through `AND`, like any `where_`:

```rust
let transaction = dinoco::find_first::<Transaction>()
    .where_(|transaction| transaction.payout.where_(|payout| payout.method.eq(PayoutMethod::Pix)))
    .where_(|transaction| transaction.amount.gt(200))
    .execute(&client)
    .await?;
```

## Filter methods

| Method | Keeps the rows that... | SQL |
| --- | --- | --- |
| `where_(...)` | have at least one related row matching the condition | `EXISTS (...)` |
| `where_complex(...)` | same, with a [where complex](/en-us/docs/orm/orm/where-complex) tree over the related row | `EXISTS (...)` |
| `none(...)` | have no related row matching the condition — rows without related rows included | `NOT EXISTS (...)` |
| `every(...)` | have only related rows matching the condition — rows without related rows included | `NOT EXISTS (... AND (condition) IS NOT TRUE)` |
| `exists()` | have at least one related row | `EXISTS (...)` |
| `not_exists()` | have no related row | `NOT EXISTS (...)` |

```rust
// Transactions with a refunded item.
dinoco::find_many::<Transaction>().where_(|t| t.items.where_(|item| item.refunded.eq(true)));

// Transactions without any refunded item, including transactions with no items.
dinoco::find_many::<Transaction>().where_(|t| t.items.none(|item| item.refunded.eq(true)));

// Transactions whose items were all refunded, including transactions with no items.
dinoco::find_many::<Transaction>().where_(|t| t.items.every(|item| item.refunded.eq(true)));

// Transactions with at least one item / without a payout.
dinoco::find_many::<Transaction>().where_(|t| t.items.exists());
dinoco::find_many::<Transaction>().where_(|t| t.payout.not_exists());
```

> [!NOTE]
> `every` and `none` are true for a row with no related rows at all. Add `.exists()` when "every item was refunded" should also require at least one item: `.where_(|t| t.items.every(...)).where_(|t| t.items.exists())`.

`every` counts a related row as matching only when its condition is `TRUE`. A comparison against a `NULL` column is neither true nor false in SQL, so an item with no `note` does **not** satisfy `every(|item| item.note.eq("gift"))` — the same row would not be returned by `find_many::<Item>().where_(|item| item.note.eq("gift"))` either.

## Every relation shape

The same methods exist on every relation field; what "related rows" means follows the relation:

| Relation | Field | Related rows |
| --- | --- | --- |
| Many-to-one | `transaction.payout` | The payout `payout_id` points at, if any |
| One-to-many | `payout.transactions` | Every transaction whose `payout_id` is this payout |
| One-to-one, owning side | `profile.user` | The user `user_id` points at, if any |
| One-to-one, other side | `user.profile` | The profile whose `user_id` is this user, if any |
| Implicit many-to-many | `business.systems` | Every system linked through the pivot table |
| Self relation | `employee.manager`, `employee.reports` | Rows of the same model, matched through the relation's keys |

```rust
// The other side of a one-to-one.
dinoco::find_many::<User>().where_(|user| user.profile.where_(|profile| profile.bio.like("rust")));

// Implicit many-to-many: filter by any field of the other side, not only its id.
dinoco::find_many::<Business>().where_(|business| business.systems.where_(|system| system.name.eq("ERP")));

// Self relation: employees managed by Ana, and managers without reports.
dinoco::find_many::<Employee>().where_(|employee| employee.manager.where_(|manager| manager.name.eq("Ana")));
dinoco::find_many::<Employee>().where_(|employee| employee.reports.not_exists());
```

For a singular relation, `where_` reads as "the related row exists and matches", and `none` as "there is no related row, or it doesn't match" — so `transaction.payout.none(|p| p.method.eq(PayoutMethod::Pix))` also returns the transactions with no payout.

## Same row or different rows

The conditions inside **one** callback must hold on the **same** related row. Separate relation filters are independent, so each one may be satisfied by a different related row:

```rust
// A single item that is refunded *and* has the "gift" note.
dinoco::find_many::<Transaction>()
    .where_(|t| t.items.where_complex(|item, m| m.and([item.refunded.eq(true), item.note.eq("gift")])));

// Some refunded item, and some (possibly other) item with the "gift" note.
dinoco::find_many::<Transaction>()
    .where_(|t| t.items.where_(|item| item.refunded.eq(true)))
    .where_(|t| t.items.where_(|item| item.note.eq("gift")));
```

## Nest relation filters

The callback receives the related model's own `Where`, relation fields included, so filters nest as deep as the schema goes:

```rust
// Transactions paid out to a Nubank account.
let transactions = dinoco::find_many::<Transaction>()
    .where_(|t| t.payout.where_(|payout| payout.bank.where_(|bank| bank.name.eq("Nubank"))))
    .execute(&client)
    .await?;

// Payouts with at least one transaction that has a refunded item.
let payouts = dinoco::find_many::<Payout>()
    .where_(|payout| payout.transactions.where_(|t| t.items.where_(|item| item.refunded.eq(true))))
    .execute(&client)
    .await?;
```

Each level becomes its own subquery, correlated with the level above it, and still runs as one statement.

## Combine with where complex

A relation filter is a regular condition, so it goes anywhere inside a [where complex](/en-us/docs/orm/orm/where-complex) tree:

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

`m.not(t.payout.where_(...))` keeps the transactions without a payout, exactly like `t.payout.none(...)`: the subquery is either true or false, never `NULL`.

## Supported builders

A relation filter works anywhere a `where_` (or `where_complex`) does:

- `find_first`, `find_many`, `find_batch`, and their `.select::<S>()`, `.pluck(...)`, and `.transform(...)` forms;
- `count` and `exists`;
- `update`, `update_many`, `find_and_update`, `delete`, and `delete_many`, `.returning::<S>()` included;
- the `where_` of relation builders inside `.includes(...)` and inside `count(...).includes(...)`;
- all of the above with `.execute(tx)` inside a [transaction](/en-us/docs/orm/orm/transactions).

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

## Relation filters versus includes

Both take a `where_` over the related model, but they answer different questions:

| | Relation filter | Include filter |
| --- | --- | --- |
| Written as | `t.items.where_(...)` inside `.where_(...)` | `t.items().where_(...)` inside `.includes(...)` |
| Decides | which **parent** rows come back | which **related** rows are loaded into each parent |
| Loads the relation | no | yes |

They compose. Return only the payouts that have a refunded transaction, and load just those transactions:

```rust
let payouts = dinoco::find_many::<Payout>()
    .where_(|payout| payout.transactions.where_(|t| t.items.where_(|item| item.refunded.eq(true))))
    .includes(|payout| payout.transactions().where_(|t| t.items.where_(|item| item.refunded.eq(true))))
    .execute(&client)
    .await?;
```

The include's own `where_` accepts relation filters too, as shown above: `t.items.where_(...)` there filters the transactions being loaded.

## Generated SQL and performance

- **One statement, no fan-out.** A relation filter is a correlated `EXISTS` subquery, not a join, so a parent with many matching children is still returned once — no duplicate rows and no `DISTINCT`. The database stops looking at a parent's related rows as soon as one decides the result, and PostgreSQL and MySQL 8 plan `EXISTS`/`NOT EXISTS` as semi-joins and anti-joins.
- **Indexed lookups.** The subquery is correlated on the relation's keys: the referenced key (usually the primary key) for a many-to-one or the owning side of a one-to-one, and the foreign key for the other direction. Dinoco [indexes every foreign key](/en-us/docs/orm/guide/indexes#foreign-keys-are-indexed) and both pivot columns of an implicit many-to-many, so each lookup is an index probe.
- **Self relations and nesting.** Each level aliases its tables by depth (`__dinoco_relation_1`, `__dinoco_relation_2`, ...), so a self relation never confuses the row being filtered with its related row.
- **Bound values.** Values inside relation filters are bound parameters, in the order they appear, like every other filter.
- **MySQL writes.** MySQL refuses an `UPDATE`/`DELETE` that reads the table being written from a subquery (error 1093) — for example `update_many::<Employee>()` filtered by `employee.manager`. Dinoco compiles exactly those filters into a derived table of the matching keys that MySQL materializes before writing, so they work unchanged. Reads, writes that don't read their own table, and the other databases keep the plain correlated subquery.
