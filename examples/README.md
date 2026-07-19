# Dinoco web examples

Both examples expose the same complete Todo CRUD API backed by Dinoco and SQLite:

- `GET /todos`
- `GET /todos/{id}`
- `POST /todos` with `{ "title": "Ship Dinoco" }`
- `PUT /todos/{id}` with `{ "title": "Release Dinoco", "completed": true }`
- `DELETE /todos/{id}`

The entities and database connection are generated from each application's `dinoco/schema.dinoco` file. Generate and apply the migration before starting an example.

Run Axum on `http://127.0.0.1:3000`:

```bash
cd examples/axum
export DATABASE_URL=axum-example.sqlite
dinoco migrate generate
cargo run
```

Run Actix Web on `http://127.0.0.1:3001`:

```bash
cd examples/actix-web
export DATABASE_URL=actix-example.sqlite
dinoco migrate generate
cargo run
```
