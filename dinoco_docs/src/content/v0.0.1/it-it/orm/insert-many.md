# insert_many

Usato per l'inserimento in batch.

---

## Cosa puoi fare

- `.values(Vec&lt;M&gt;)`: definisce i record che verranno inseriti in batch.
- `.with_relation(Vec&lt;R&gt;)`: inserisce esattamente un nuovo correlato per ogni padre.
- `.with_relations(Vec&lt;Vec\<R&gt;\>)` : inserisce più nuovi correlati per ogni padre.
- `.with_connection(Vec&lt;R&gt;)`: connette esattamente una relazione già esistente per ogni padre inserito.
- `.with_connections(Vec&lt;Vec\<R&gt;\>)` : connette più relazioni già esistenti per ogni padre inserito, tipicamente in scenari many-to-many.
- `.returning::&lt;T&gt;()`: cambia il ritorno a una lista tipizzata degli elementi inseriti.
- `.execute(&client)`: esegue l'operazione in batch.

## Ritorno

Senza `.returning::&lt;T&gt;()`, il ritorno è:

```rust
DinocoResult<()>
```

Con `.returning::&lt;T&gt;()`, il ritorno diventa:

```rust
DinocoResult<Vec<T>>
```

## Esempio semplice

```rust
dinoco::insert_many::<User>()
    .values(vec![
        User { id: "u1".into(), email: "a@acme.com".into(), name: "A".into() },
        User { id: "u2".into(), email: "b@acme.com".into(), name: "B".into() },
    ])
    .execute(&client)
    .await?;
```

## Esempio con ritorno

```rust
let created = dinoco::insert_many::<User>()
    .values(vec![
        User { id: 2, name: "Anna".to_string() },
        User { id: 3, name: "Caio".to_string() },
    ])
    .returning::<User>()
    .execute(&client)
    .await?;
```

## Esempio con with_relation(...)

Usa `with_relation(...)` quando ogni padre deve essere creato e connesso.

```rust
dinoco::insert_many::<Post>()
    .values(vec![
        Team { id: "team-11".into(), name: "Dati".into() },
        Team { id: "team-12".into(), name: "DX".into() },
    ])
    .with_relation(vec![
        Member { id: "member-11".into(), name: "Raffa".into(), teamId: "legacy".into() },
        Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
    ])
    .execute(&client)
    .await?;
```

## Esempio con with_relations(...)

Usa with_relations(...) quando ogni record padre ha più di un figlio; i record verranno creati e collegati automaticamente.

```rust
dinoco::insert_many::<Post>()
    .values(vec![
        Team { id: "team-11".into(), name: "Dati".into() },
        Team { id: "team-12".into(), name: "DX".into() },
    ])
    .with_relation(vec![
        vec![
			Member { id: "member-11".into(), name: "Raffa".into(), teamId: "legacy".into() },
        	Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
		],

		vec![Member { id: "member-11".into(), name: "Raffa".into(), teamId: "legacy".into() }]
    ])
    .execute(&client)
    .await?;
```

## Esempio con with_connection(...)

Usa `with_connection(...)` quando ogni padre deve essere connesso a esattamente un record già esistente.

```rust
dinoco::insert_many::<Team>()
    .values(vec![
        Team { id: "team-11".into(), name: "Dati".into() },
        Team { id: "team-12".into(), name: "DX".into() },
    ])
    .with_connection(vec![
        Member { id: "member-11".into(), name: "Raffa".into(), teamId: "legacy".into() },
        Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
    ])
    .execute(&client)
    .await?;
```

## Esempio con with_connections(...)

Usa `with_connections(...)` quando ogni padre deve essere connesso a più record già esistenti.
Questo è un uso comune nelle relazioni many-to-many.

```rust
dinoco::insert_many::<Article>()
    .values(vec![
        Article { id: "article-11".into(), title: "Connetti Multipli".into() },
        Article { id: "article-12".into(), title: "Connetti Batch".into() },
    ])
    .with_connections(vec![
        vec![
            Label { id: "label-11".into(), name: "orm".into() },
            Label { id: "label-12".into(), name: "rust".into() },
        ],
        vec![
            Label { id: "label-10".into(), name: "backend".into() },
        ],
    ])
    .execute(&client)
    .await?;
```

## Note

- `with_relation(...)` e `with_relations(...)` funzionano meglio quando il padre ha già un ID conosciuto in memoria.
- `with_connection(...)` e `with_connections(...)` sono pensati per la connessione di relazioni già esistenti.
- `with_connection(...)` richiede lo stesso numero di elementi di `values(...)`.
- `with_connections(...)` richiede lo stesso numero di gruppi rispetto al vettore principale.

## Prossimi passi

- [**`insert_into::&lt;M&gt;()`**](/v0.0.1/orm/insert-into): inserimento singolo.
- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): aggiornamento con filtro.
