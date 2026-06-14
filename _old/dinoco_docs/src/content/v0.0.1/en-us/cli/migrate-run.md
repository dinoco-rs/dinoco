# How to use?

The `dinoco migrate run` command executes all pending migrations.

It is used to align the configured database with the already generated migration history.

---

## What the command does

When executing this command, the CLI:

- Reads the migration history
- Checks which ones have not yet been applied
- Executes the pending migrations in order

## When to use

Use this command when:

- You already have generated migrations
- You want to apply pending changes to the database
- You are preparing a local or deployment environment

## Next steps

If necessary, generate the models with:

```bash
dinoco models generate
```
