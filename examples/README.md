# Dinoco web examples

Three runnable services expose the **same JSON API** on top of Dinoco + SQLite,
one per HTTP stack:

| Example | Stack | Port |
| --- | --- | --- |
| [`axum`](./axum) | Axum 0.8 | `3000` |
| [`actix-web`](./actix-web) | Actix Web 4 | `3001` |
| [`tower`](./tower) | Raw `tower` + `hyper` 1 (hand-written `Layer`, `ConcurrencyLimit`, `Timeout`) | `3002` |

## The domain

A `Project` owns many `Task` rows (`one_to_many` / `many_to_one`). The handlers
are deliberately one Dinoco call each so every builder shows up somewhere:

| Route | Dinoco surface |
| --- | --- |
| `POST /projects` | **`transaction`** — insert the project and its initial tasks atomically |
| `GET /projects` | **`find_many`** + `order_by` + eager **`includes`** of `tasks` |
| `GET /projects/{id}` | **`find_first`** + `includes`, plus a **`count`** of open tasks |
| `PATCH /projects/{id}` | **`update`** … `returning` |
| `DELETE /projects/{id}` | **`transaction`** — delete the tasks, then the project |
| `POST /projects/{id}/tasks` | **`insert_into`** … `returning` |
| `GET /tasks?project_id=&done=` | **`find_many`** with one optional `where_` per filter, `take(100)` |
| `GET /tasks/{id}` | **`find_first`** |
| `PATCH /tasks/{id}` | **`find_and_update`** (atomic; `RowNotAffected` → `404`) |
| `DELETE /tasks/{id}` | **`delete`** … `returning` |

Request bodies:

- `POST /projects` — `{ "name": "Launch", "tasks": ["spec", "build", "ship"] }`
- `PATCH /projects/{id}` — `{ "name": "Launch v2", "archived": true }` (any subset)
- `POST /projects/{id}/tasks` — `{ "title": "write docs" }`
- `PATCH /tasks/{id}` — `{ "title": "…", "done": true }` (any subset)

## Running one

The entities and the database connection in each `dinoco/` folder are generated
from that app's `dinoco/schema.dinoco`. Generate and apply the migration, then
start the server:

```bash
cd examples/axum          # or actix-web, or tower
export DATABASE_URL=example.sqlite
dinoco migrate generate   # creates + applies the migration, regenerates models
cargo run
```

```bash
# smoke test (axum on :3000)
curl -s localhost:3000/projects \
  -H 'content-type: application/json' \
  -d '{"name":"Launch","tasks":["spec","build","ship"]}'

curl -s localhost:3000/projects
```
