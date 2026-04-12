# find_and_update

Usato per localizzare un singolo record, applicare aggiornamenti atomici nel database e restituire l'elemento aggiornato.

---

## Cosa puoi fare

- `.cond(...)`: definisce quale record verrà localizzato.
- `.update(...)`: applica un'operazione atomica su un campo del modello.
- `.execute(&client)`: esegue l'aggiornamento e restituisce il record aggiornato.

## Ritorno

Il ritorno di `find_and_update` è:

```rust
DinocoResult<M>
```

## Esempio base

```rust
let task = dinoco::find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .execute(&client)
    .await?;
```

## Esempio con worker

```rust
use database::*;

let _worker = workers()
    .on::<Task, _, _>("task.reviewed", |job| async move {
        // Task aggiornata a
        println!("Task aggiornata a {:?}", job.data.status);
        job.success();
    })
    .run()
    .await?;

let task = dinoco::find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .enqueue("task.reviewed")
    .execute(&client)
    .await?;
```

Vedi di più sui worker in [**`queues`**](/v0.0.2/orm/queues).

## Operazioni disponibili in `ModelUpdate`

- `set(value)`
- `increment(value)`
- `decrement(value)`
- `multiply(value)`
- `division(value)`

## Osservazioni

- L'aggiornamento viene eseguito in un singolo `UPDATE`.
- Se nessuna riga corrisponde alla condizione, il ritorno sarà `DinocoError::RecordNotFound`.
- La DSL di aggiornamento non espone relazioni.
- Attualmente il flusso supporta una chiave primaria semplice per localizzare e restituire il record aggiornato.

## Prossimi passi

- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): aggiornamento tradizionale.
- [**`update_many::&lt;M&gt;()`**](/v0.0.1/orm/update-many): aggiornamento in batch.
