# update

Atualiza os registros que correspondem a uma condição. Uma chamada a `.cond(...)` é obrigatória antes de executar o update.

---

## O que você pode fazer

- `.cond(...)`: define os registros que serão atualizados. Obrigatório.
- `.update(...)`: altera um campo; pode ser chamado várias vezes.
- `.connect(...)` e `.disconnect(...)`: criam ou removem vínculos de relações suportadas para escrita.
- `.returning()`: retorna os models atualizados.
- `.returning_as::<T>()`: retorna os registros atualizados em uma projeção tipada.
- `.execute(&client)`: executa uma única instrução `UPDATE`.

`values(...)` não faz parte da API de update. Você informa somente os campos que quer mudar.

## Exemplo

```rust
dinoco::update::<User>()
    .cond(|user| user.id.eq(10))
    .update(|user| user.email.set("novo@acme.com"))
    .update(|user| user.name.set("Novo Nome"))
    .execute(&client)
    .await?;
```

Todos os `.update(...)` encadeados são aplicados na mesma operação.

## Operações numéricas

Campos numéricos também aceitam operações incrementais:

```rust
dinoco::update::<Product>()
    .cond(|product| product.id.eq(product_id))
    .update(|product| product.stock.decrement(1_i64))
    .update(|product| product.sales.increment(1_i64))
    .execute(&client)
    .await?;
```

## Retorno tipado

```rust
let updated = dinoco::update::<User>()
    .cond(|user| user.id.eq(10))
    .update(|user| user.name.set("Novo Nome"))
    .returning_as::<UserSummary>()
    .execute(&client)
    .await?;
```

## Relações para escrita

```rust
dinoco::update::<User>()
    .cond(|user| user.id.eq(10))
    .connect(|relations| relations.roles().slug.eq("admin"))
    .execute(&client)
    .await?;
```

## Próximos passos

- [**`update_many::<M>()`**](/v0.1.1/orm/update-many): atualiza vários registros.
- [**`find_and_update::<M>()`**](/v0.1.1/orm/find-and-update): atualiza e retorna um único registro.
