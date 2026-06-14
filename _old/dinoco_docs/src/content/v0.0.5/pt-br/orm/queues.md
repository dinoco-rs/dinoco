# queues

O sistema de filas do Dinoco permite publicar eventos assíncronos a partir de `insert`, `find` e `update`, guardando no `Redis` apenas o dado necessário para re-hidratar o registro quando o worker for executar.

## O que você pode fazer

- Permitir o encadeamento de .enqueue(...), .enqueue_in(...) e .enqueue_at(...) em todos os métodos do cliente, exceto nas operações de exclusão.
- Processar jobs com `workers().on::&lt;Model&gt;(... )`.
- Carregar projeções com relações no worker usando `workers().on_with_relation::&lt;Model, Projection&gt;(... )`.
- Recarregar o dado mais recente antes do handler rodar.
- Ignora automaticamente jobs cujo registro não existe mais.

## Como funciona

Quando você usa `enqueue`, o Dinoco:

1. Executa a operação principal normalmente.
2. Salva na fila apenas os critérios de busca do registro.
3. No worker, busca os dados mais recentes no banco.
4. Só então executa o handler.

Se o registro não existir mais no momento da busca, o handler não roda e o job é removido da fila.

## Métodos de enfileiramento

- `.enqueue(evento)`: agenda para execução imediata.
- `.enqueue_in(evento, delay_ms)`: agenda com atraso em milissegundos.
- `.enqueue_at(evento, data_utc)`: agenda para uma data específica em `chrono::DateTime&lt;Utc&gt;`.

```rust
use database::*;

insert_into::<User>()
    .values(User { id: 1, name: "Matheus".to_string() })
    .enqueue("user.created")
    .execute(&client)
    .await?;

insert_into::<User>()
    .values(User { id: 2, name: "Ana".to_string() })
    .enqueue_in("user.reminder", 30_000)
    .execute(&client)
    .await?;

insert_into::<User>()
    .values(User { id: 3, name: "Lia".to_string() })
    .enqueue_at("user.scheduled", dinoco::Utc::now() + chrono::Duration::hours(1))
    .execute(&client)
    .await?;
```

## Worker

Use `workers()` para registrar handlers assíncronos. O contexto do worker expõe:

- `data`: o registro re-hidratado.
- `client`: uma cópia do `DinocoClient`.
- `success()` ou `remove()`: remove o job da fila.
- `fail()`: reprograma com o retry padrão.
- `retry_in()` e `retry_at()`: mesma ideia de `enqueue_in` e `enqueue_at`.
- `.run().await?`: inicia o worker em background e retorna imediatamente com um `JoinHandle`.

Ao chamar `run`, o Dinoco cria um novo `DinocoClient` exclusivo para os workers, com pool próprio de conexões. Assim o loop da fila não reaproveita o mesmo pool da aplicação principal.

```rust
use database::*;

let _worker = workers()
    .on::<User, _, _>("user.created", |job| async move {
        println!("Processando {}", job.data.name);
        job.success()
    })
    .run()
    .await?;
```

Se você precisar de uma projeção com relações, use `on_with_relation`. O Dinoco recarrega o registro e aplica a árvore de includes antes do handler rodar.

```rust
use database::*;

let _worker = workers()
    .on_with_relation::<User, UserWithPosts, _, _, _, _>(
        "user.created",
        |user| user.posts().select::<PostListItem>(),
        |job| async move {
            println!("{} tem {} posts", job.data.name, job.data.posts.len());
            job.success()
        },
    )
    .run()
    .await?;
```

Você também pode trabalhar com listas:

```rust
use database::*;

let _worker = workers()
    .on::<Vec<User>, _, _>("user.batch-updated", |job| async move {
        for user in job.data {
            println!("{}", user.name);
        }

        job.success()
    })
    .run()
    .await?;
```

## Configuração do Redis

As filas usam o `Redis` configurado no `DinocoClientConfig::with_redis(...)`. Para `enqueue_in` e `enqueue_at`, é aconselhável habilitar persistência, porque jobs agendados vivem no Redis até o horário de execução.

Recomendação: use **AOF + RDB juntos**.

```conf
# Ativa AOF
appendonly yes

# Frequência de fsync
appendfsync everysec

# Arquivo AOF
appendfilename "appendonly.aof"

# --- SNAPSHOT (RDB) ---
save 900 1
save 300 10
save 60 10000

# --- SEGURANÇA ---
dir /data

# --- PERFORMANCE ---
no-appendfsync-on-rewrite yes
```

## Observações

- `delete` e `delete_many` não possuem suporte a `enqueue`.
- O worker é totalmente assíncrono e usa `tokio` por baixo dos panos
- `run()` não trava o restante da aplicação: ele sobe o loop em background.
- `Option&lt;User&gt;` é aceito no registro do worker, mas o handler só roda quando houver registro para re-hidratar.

## Próximos passos

- [**`insert_into::&lt;M&gt;()`**](/v0.0.2/orm/insert-into)
- [**`find_many::&lt;M&gt;()`**](/v0.0.2/orm/find-many)
- [**`update::&lt;M&gt;()`**](/v0.0.2/orm/update)
