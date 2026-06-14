# find_and_update

Utilisé pour localiser un seul enregistrement, appliquer des mises à jour atomiques dans la base de données et retourner l'élément mis à jour.

---

## Ce que vous pouvez faire

- `.cond(...)`: définit quel enregistrement sera localisé.
- `.update(...)`: applique une opération atomique sur un champ du modèle.
- `.execute(&client)`: exécute la mise à jour et retourne l'enregistrement mis à jour.

## Retour

Le retour de `find_and_update` est :

```rust
DinocoResult<M>
```

## Exemple de base

```rust
let task = dinoco::find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .execute(&client)
    .await?;
```

## Exemple avec un worker

```rust
use database::*;

let _worker = workers()
    .on::<Task, _, _>("task.reviewed", |job| async move {
        // Tâche mise à jour vers
        println!("Tâche mise à jour vers {:?}", job.data.status);
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

Voir plus sur les workers dans [**`queues`**](/v0.0.2/orm/queues).

## Opérations disponibles dans `ModelUpdate`

- `set(value)`
- `increment(value)`
- `decrement(value)`
- `multiply(value)`
- `division(value)`

## Observations

- La mise à jour est exécutée en un seul `UPDATE`.
- Si aucune ligne ne correspond à la condition, le retour sera `DinocoError::RecordNotFound`.
- La DSL de mise à jour n'expose pas les relations.
- Actuellement, le flux supporte une clé primaire simple pour localiser et retourner l'enregistrement mis à jour.

## Prochaines étapes

- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): mise à jour traditionnelle.
- [**`update_many::&lt;M&gt;()`**](/v0.0.1/orm/update-many): mise à jour par lot.
