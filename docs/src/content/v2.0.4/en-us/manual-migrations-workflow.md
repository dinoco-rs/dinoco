# Manual migration workflow

With `migration_engine = "manual"`, the migration commands change meaning slightly. `dinoco models generate` behaves the same in both engines.

## dinoco migrate generate

```bash
dinoco migrate generate create_users
```

Creates `dinoco/migrations/<timestamp>_create_users.rs`, registers it in `dinoco/migrations/mod.rs`, and regenerates the Rust models. It does **not** touch the database, so it needs no `DATABASE_URL`. If you leave the name off, the CLI asks for it. The name is normalized to `snake_case`; a name with no letters or digits is rejected.

Fill in `up` and `down` before running the migration. A freshly scaffolded migration compiles and does nothing.

## dinoco migrate run

```bash
dinoco migrate run
```

Applies every pending migration, oldest first, and records each one in `dinoco_manual_migrations` after its `up` succeeds. Running it again with nothing pending prints `No pending migrations.` This is the command for a deploy pipeline. It needs the database URL.

## dinoco migrate rollback

```bash
dinoco migrate rollback            # reverts the latest applied migration
dinoco migrate rollback --steps 3  # reverts the latest three
```

Runs `down` for the newest applied migrations, newest first, and removes each from the history after its `down` succeeds. Asking for more steps than were applied simply stops at the oldest one. `--steps 0` is rejected.

## dinoco migrate status

```bash
dinoco migrate status
```

```text
[applied] 20260919120000_create_users
[pending] 20260920093000_add_user_email
```

`status` fails if the database has an applied migration that is missing from `mod.rs`. That means a file was deleted or renamed after being applied.

All four commands accept `--workspace name`/`-w name`. `rollback` and `status` exist only for the manual engine: on an `automatic` schema they stop with a message asking for `migration_engine = "manual"`.

## How migrations are executed

The CLI is a prebuilt program and cannot load your Rust files. To run a migration it generates a tiny Cargo project in `dinoco/.runner/`, whose `main.rs` includes your `dinoco/migrations/mod.rs`, and runs it with `cargo run`. The database settings reach it through environment variables set by the CLI, so the URL never appears on a command line.

What that means in practice:

- A Rust toolchain must be available where you run `dinoco migrate run` — a CI job that runs migrations needs `cargo`.
- The first run compiles Dinoco into `dinoco/.runner/target/` and takes a while; later runs only rebuild what changed. The directory ignores itself (it contains its own `.gitignore`), so nothing needs to be added to yours.
- The runner depends on the same published Dinoco version as the CLI. Migration files can therefore use `::dinoco` (and `std`), but not your application's other modules or crates. Put that logic in SQL, or run the registry from your own binary as shown in the [manager reference](/en-us/docs/orm/guide/migration-manager#running-a-registry-from-code).
- A compile error in a migration is printed by Cargo, followed by `the Dinoco runner failed`. No migration runs when the runner fails to build.
- You can delete `dinoco/.runner/` at any time; it is recreated on demand.

## Workspaces

With workspaces, each workspace keeps its own migrations in `dinoco/migrations/<workspace>/` (including its own `mod.rs`), and the generated `dinoco/mod.rs` points `pub mod migrations;` at the selected one:

```bash
dinoco migrate generate create_users --workspace dev
dinoco migrate run -w dev
```

## Recommended workflow

```bash
# One time
dinoco init --migration-engine manual
dinoco migrate generate create_users
# edit dinoco/migrations/<timestamp>_create_users.rs: write `up` and `down`

dinoco migrate run          # apply it
dinoco migrate rollback     # try the `down`
dinoco migrate run          # and apply again

# In CI / deploy
dinoco migrate run
```

Keep `schema.dinoco` in step with what your migrations build: the models are generated from the schema, not from the migrations, and Dinoco does not compare the two in manual mode.

## Troubleshooting

| Message | Cause |
| --- | --- |
| `dinoco/migrations/mod.rs was not found` | Nothing was scaffolded yet — run `dinoco migrate generate <name>` |
| `is missing the ... marker` | The marker comments in `mod.rs` were removed; restore them or register the migration by hand |
| `was applied but is not registered` | An applied migration was deleted, renamed, or dropped from `migrations()` |
| `is registered more than once` | The same name appears twice in `migrations()` |
| `migration ... failed while applying up` | Your `up` returned an error; it was not recorded, fix it and re-run |
| `failed to launch cargo` | No Rust toolchain on `PATH` |
