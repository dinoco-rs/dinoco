# find_first

Usato per cercare al massimo un record.

---

## Cosa puoi fare

Espone gli stessi metodi di `find_many`:

- `select`
- `cond`
- `take`
- `skip`
- `order_by`
- `includes`
- `count`
- `read_in_primary`
- `execute`

## Descrizione dei metodi

- `.select::&lt;T&gt;()`: scambia la proiezione predefinita con una proiezione personalizzata.
- `.cond(...)`: aggiunge filtri alla ricerca.
- `.take(...)`: limita la quantità massima di record considerati.
- `.skip(...)`: salta i record prima di selezionare il primo risultato.
- `.order_by(...)`: definisce quale record deve essere considerato per primo.
- `.includes(...)`: carica le relazioni insieme all'elemento principale.
- `.count(...)`: calcola i contatori delle relazioni nella proiezione.
- `.read_in_primary()`: forza la lettura nel database principale.
- `.execute(&client)`: esegue la query e restituisce al massimo un elemento.

## Ritorno

Senza `select::&lt;T&gt;()`, il ritorno è:

```rust
DinocoResult<Option<M>>
```

Con `select::&lt;T&gt;()`, il ritorno diventa:

```rust
DinocoResult<Option<T>>
```

## Esempio base

```rust
let user = dinoco::find_first::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

## Esempio con select

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: i64,
    name: String,
}

let user = dinoco::find_first::<User>()
    .select::<UserSummary>()
    .cond(|w| w.id.eq(1_i64))
    .execute(&client)
    .await?;
```

## Esempio con relazione

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let user = dinoco::find_first::<User>()
    .select::<UserWithPosts>()
    .cond(|x| x.id.eq(1_i64))
    .includes(|x| x.posts())
    .execute(&client)
    .await?;
```

## Esempio con ordinamento

```rust
let latest_user = dinoco::find_first::<User>()
    .order_by(|x| x.id.desc())
    .execute(&client)
    .await?;
```

## Prossimi passi

- [**`find_many::&lt;M&gt;()`**](/v0.0.1/orm/find-many): cerca più record.
- [**`count::&lt;M&gt;()`**](/v0.0.1/orm/count): conteggio dei record.
