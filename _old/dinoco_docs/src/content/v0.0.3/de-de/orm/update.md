# update

Wird verwendet, um gefilterte Datensätze zu aktualisieren.

---

## Was Sie tun können

- `.cond(...)`: definiert, welche Datensätze aktualisiert werden sollen.
- `.values(item)`: gibt die neuen Werte des Datensatzes an.
- `.connect(...)`: erstellt unterstützte Beziehungsverknüpfungen zum Schreiben.
- `.disconnect(...)`: entfernt unterstützte Beziehungsverknüpfungen zum Schreiben.
- `.returning::&lt;T&gt;()`: gibt die aktualisierten Datensätze in einer typisierten Projektion zurück.
- `.execute(&client)`: führt die Aktualisierung in der Datenbank aus.

## Rückgabe

Ohne `.returning::&lt;T&gt;()` ist die Rückgabe:

```rust
DinocoResult<()>
```

Mit `.returning::&lt;T&gt;()` wird die Rückgabe:

```rust
DinocoResult<Vec<T>>
```

Hinweis:

- `update().returning()` unterstützt keine Beziehungs-Schreibvorgänge mit `.connect(...)` oder `.disconnect(...)`.

## Beispiel für die Aktualisierung von Feldern

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .values(User {
        id: 10,
        email: "novo@acme.com".to_string(),
        name: "Neuer Name".to_string(),
    })
    .execute(&client)
    .await?;
```

## Beispiel mit connect(...)

Wird verwendet, um unterstützte Beziehungen zum Schreiben zu verbinden, normalerweise Many-to-Many.

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .connect(|r| r.roles().slug.eq("admin"))
    .execute(&client)
    .await?;
```

## Beispiel mit disconnect(...)

Wird verwendet, um Beziehungen zu trennen.

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .disconnect(|r| r.roles().slug.eq("guest"))
    .execute(&client)
    .await?;
```

## Beispiel mit Worker

```rust
use database::*;

let _worker = workers()
    .on::<User, _, _>("user.updated", |job| async move {
        println!("Benutzer aktualisiert: {}", job.data.name);
        job.success();
    })
    .run()
    .await?;

dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .values(User {
        id: 10,
        email: "novo@acme.com".to_string(),
        name: "Neuer Name".to_string(),
    })
    .enqueue("user.updated")
    .execute(&client)
    .await?;
```

Erfahren Sie mehr über Worker unter [**`queues`**](/v0.0.2/orm/queues).

## Verfügbare Filter in connect und disconnect

- `eq`
- `neq`
- `gt`
- `gte`
- `lt`
- `lte`
- `in_values`
- `not_in_values`
- `is_null`
- `is_not_null`
- `includes`
- `starts_with`
- `ends_with`

## Nächste Schritte

- [**`update_many::&lt;M&gt;()`**](/v0.0.1/orm/update-many): Stapelaktualisierung.
- [**`find_and_update::&lt;M&gt;()`**](/v0.0.1/orm/find-and-update): atomares Update mit Rückgabe.
