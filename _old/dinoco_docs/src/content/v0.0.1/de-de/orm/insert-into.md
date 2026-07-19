# insert_into

Wird verwendet, um einen Datensatz einzufügen.

---

## Was Sie tun können

- Werte mit `.values(item)` übergeben
- Mit Beziehung über `.with_relation(related)` einfügen
- Bestehende Beziehung über `.with_connection(connected)` verbinden
- Mit `.execute(&client)` ausführen

## Beschreibung der Methoden

- `.values(item)`: Definiert den Datensatz, der eingefügt werden soll.
- `.with_relation(related)`: Fügt einen neuen verwandten Datensatz zusammen mit dem Elternteil ein.
- `.with_connection(connected)`: Verbindet den eingefügten Datensatz mit einem bereits vorhandenen Element, normalerweise in Beziehungsflüssen, die für die Verbindung unterstützt werden.
- `.returning::&lt;T&gt;()`: Ändert den Rückgabewert zu einer typisierten Projektion des eingefügten Elements.
- `.execute(&client)`: Führt den Schreibvorgang in die Datenbank aus.

## Rückgabe

Ohne `.returning::&lt;T&gt;()` ist der Rückgabewert:

```rust
DinocoResult<()>
```

Mit `.returning::&lt;T&gt;()` wird der Rückgabewert:

```rust
DinocoResult<T>
```

## Einfaches Beispiel

```rust
dinoco::insert_into::<User>()
    .values(User {
        id: "usr_1".to_string(),
        email: "ana@acme.com".to_string(),
        name: "Ana".to_string(),
    })
    .execute(&client)
    .await?;
```

## Beispiel mit Beziehung

Verwenden Sie `.with_relation(...)`, wenn das generierte Modell das Einfügen des Elternteils und des Verwandten zusammen unterstützt.

```rust
dinoco::insert_into::<User>()
    .values(user)
    .with_relation(profile)
    .execute(&client)
    .await?;
```

## Beispiel mit Verbindung

Verwenden Sie `.with_connection(...)`, wenn Sie ein Element einfügen und eine bereits bestehende Beziehung verbinden möchten.

```rust
dinoco::insert_into::<User>()
    .values(new_user)
    .with_connection(existing_team)
    .execute(&client)
    .await?;
```

## Beispiel mit typisiertem Rückgabewert

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: i64,
    name: String,
}

let created = dinoco::insert_into::<User>()
    .values(User { id: 1, name: "Matheus".to_string() })
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

## Nächste Schritte

- [**`insert_many::&lt;M&gt;()`**](/v0.0.1/orm/insert-many): Batch-Einfügung.
- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): Aktualisierung von Datensätzen.
