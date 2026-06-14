# How to use?

The `dinoco migrate generate` command generates a migration from the current schema.

It compares the current schema state with the known history and creates the necessary artifacts to evolve the database.

---

## What the command does

This command:

- Reads the current schema
- Generates a new local migration
- Prepares the artifacts used by Dinoco for database evolution

Optionally, it can also apply the migration immediately and generate the Rust models.

## Parameters

### --apply

Applies the generated migration immediately and also generates the Rust models.

Example:

```bash
dinoco migrate generate --apply
```

## Example usage without applying

```bash
dinoco migrate generate
```

This flow is useful when you want to:

- Inspect the migration before applying
- Review changes in version control
- Separate generation and execution into different steps

## Example usage with immediate application

```bash
dinoco migrate generate --apply
```

This flow is useful when you want to:

- Quickly update the local database
- Generate models right after migration
- Iterate faster during development

## Next steps

After generation, you can:

```bash
dinoco migrate run
```

or:

```bash
dinoco models generate
```
