# delete

Utilizzato per eliminare con filtro esplicito.

---

## Cosa puoi fare

- `.cond(...)`: definisce quale record verrà rimosso.
- `.execute(&client)`: esegue la rimozione nel database.

## Ritorno

Il valore di ritorno di `delete` è:

```rust
DinocoResult<()>
```

## Esempio base

```rust
dinoco::delete::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

## Esempio con un altro filtro

```rust
dinoco::delete::<Session>()
    .cond(|w| w.token.eq("session-1"))
    .execute(&client)
    .await?;
```

## Prossimi passi

- [**`delete_many::&lt;M&gt;()`**](/v0.0.1/orm/delete-many): rimozione in batch.
- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): validare i record prima di rimuovere.
