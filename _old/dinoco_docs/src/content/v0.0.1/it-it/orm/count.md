# count

Usato per contare i record.

---

## Cosa puoi fare

- Filtrare con `.cond(...)`
- Eseguire con `.execute(&client)`

## Descrizione dei metodi

- `.cond(...)`: limita quali record sono inclusi nel conteggio.
- `.execute(&client)`: esegue il conteggio nel database.

## Ritorno

Il valore di ritorno di `count` è:

```rust
DinocoResult<usize>
```

## Esempio base

```rust
let total = dinoco::count::<User>()
    .execute(&client)
    .await?;
```

## Esempio con filtro booleano

```rust
let total = dinoco::count::<User>()
    .cond(|w| w.active.eq(true))
    .execute(&client)
    .await?;
```

## Esempio con filtro di testo

```rust
let total = dinoco::count::<User>()
    .cond(|w| w.name.includes("Ana"))
    .execute(&client)
    .await?;
```

## Passi successivi

- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): recupera i record in un elenco.
- [**`find_first::&lt;M&gt;()`**](/v0.0.1/orm/find-first): recupera un singolo record.
