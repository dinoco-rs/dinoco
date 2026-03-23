# etanol-rs

⚠️ **Project Status: in development**

**etanol-rs** is currently in the **development stage**.
The API is unstable and features may change frequently.

The goal of this project is to build a **high-performance,
distributed-first ORM for Rust**, inspired by modern developer tools but
designed for **scalable architectures with sharding and replication
built in**.

> ⚡ Fueling distributed databases in Rust.

---

⭐ **If you like this project or want to support its development,
consider sponsoring it.**

❤️ GitHub Sponsors
https://github.com/sponsors/etanol-rs

<a href="https://www.buymeacoffee.com/theuszastro" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" style="height: 60px !important;width: 217px !important;" ></a>

Your support helps maintain the project and accelerate development.

---

# Vision

Most ORMs are designed for **single database instances**.

**etanol-rs** aims to provide a **modern data layer designed for
distributed systems**, including:

- native sharding
- replica routing
- distributed ID generation
- multi-database support
- type-safe queries

---

# Planned Features

## Type-Safe ORM

Define your schema and generate a type-safe Rust client.

Example schema concept:

```prisma
table User {
	id String @id @default(snowflake())

	name String @default("Anonymous")
	age Integer? @default(10)

	reference String

	isAdmin Boolean @default(false)
}
```

Example query concept:

```rust
let users = client
    .user()
    .find_many(user::email::equals("user@email.com"))
    .await?;
```

---

## Native Sharding

etanol-rs will support **automatic shard routing**.

```prisma
IN DEVELLOPMENT
```

Queries will automatically route to the correct shard.

---

## Replica Routing

Automatic **read/write split**:

    write → primary
    read  → replicas

Benefits:

- improved performance
- horizontal scaling
- high availability

---

## Distributed ID Generation

etanol-rs will include a distributed ID generator similar to Snowflake.

Example ID structure:

    timestamp | node | sequence

Advantages:

- globally unique IDs
- time sortable
- no database roundtrip

---

## Multi-Database Support (planned)

Initial targets:

- PostgreSQL
- SQLite

Later expansions:

- MySQL
- ScyllaDB
- Cassandra
- MongoDB

# Contributing

This project is still early, but contributions and ideas are welcome.

You can help by:

- discussing architecture
- opening issues
- suggesting features
- sharing use cases

---

# Support the Project

Open-source infrastructure takes time to build and maintain.

If you want to support the development of **etanol-rs**, you can
contribute here:

❤️ GitHub Sponsors
https://github.com/sponsors/theuszastro

<a href="https://www.buymeacoffee.com/theuszastro" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-violet.png" alt="Buy Me A Coffee" style="height: 60px !important;width: 217px !important;" ></a>

Your support helps:

- accelerate development
- improve documentation
- maintain the project long-term

---

# License

MIT License.

---

# ⭐ If you like the project

Consider:

- starring the repository
- sharing it with others
- sponsoring development

Even small support helps keep open source sustainable ❤️
