# count

Wird verwendet, um Datensätze zu zählen.

---

## Was Sie tun können

- Filtern mit `.cond(...)`
- Ausführen mit `.execute(&client)`

## Beschreibung der Methoden

- `.cond(...)`: schränkt ein, welche Datensätze in die Zählung einbezogen werden.
- `.execute(&client)`: führt die Zählung in der Datenbank aus.

## Rückgabe

Die Rückgabe von `count` ist:

```rust
DinocoResult<usize>
```

## Grundlegendes Beispiel

```rust
let total = dinoco::count::<User>()
    .execute(&client)
    .await?;
```

## Beispiel mit booleschem Filter

```rust
let total = dinoco::count::<User>()
    .cond(|w| w.active.eq(true))
    .execute(&client)
    .await?;
```

## Beispiel mit Textfilter

```rust
let total = dinoco::count::<User>()
    .cond(|w| w.name.includes("Ana"))
    .execute(&client)
    .await?;
```

## Nächste Schritte

- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): ruft Datensätze in einer Liste ab.
- [**`find_first::&lt;M&gt;()`**](/v0.0.1/orm/find-first): ruft einen einzelnen Datensatz ab.
