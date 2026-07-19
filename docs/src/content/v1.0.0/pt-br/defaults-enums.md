# Defaults e enums

Use defaults no schema para regras do banco e identificadores gerenciados pela lib. Esses fields também deixam de aparecer nos parâmetros de `new()`.

## Defaults literais

```dinoco
model Feature {
    id      Integer @id @default(autoincrement())
    name    String
    enabled Boolean @default(false)
    weight  Float   @default(1.0)
}
```

O valor deve ser compatível com o tipo. A migration conserva o default no banco, inclusive para inserts feitos fora do Dinoco.

## Valores gerados

As funções suportadas são `autoincrement()` em `Integer`, `uuid()` em `String`, `snowflake()` em `Integer` e `now()` em `DateTime` ou `Date`. Usar uma combinação inválida falha na compilação do schema.

## UUID

```dinoco
id String @id @default(uuid())
```

O Rust usa `dinoco::Uuid`. O valor é criado antes dos inserts relacionados, permitindo propagar a chave para relações aninhadas.

## Snowflake

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
    read_réplicas = []
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

model Event {
    id Integer @id @default(snowflake())
}
```

O field vira `dinoco::Snowflake`, baseado em `i64`.

## Autoincremento

```dinoco
id Integer @id @default(autoincrement())
```

O banco cria o inteiro. O adapter recupera a chave quando o insert precisa retorná-la ou vinculá-la a uma relação aninhada.

## Enums

```dinoco
enum Role {
    USER
    ADMIN
}

model User {
    id   Integer @id @default(autoincrement())
    role Role    @default(USER)
}
```

As variantes Rust usam PascalCase, enquanto o valor persistido mantém a grafia do schema:

```rust
pub enum Role {
    User,
    Admin,
}
```

PostgreSQL usa enum nativo, MySQL usa sua representação de enum e SQLite guarda uma representação escalar compatível. A API Rust permanece igual.

## Alterando enums com segurança

O planner detecta enums criados, alterados e removidos. Adicionar um valor costuma ser aditivo; remover ou renomear pode invalidar rows existentes. Quando há risco de perda, a CLI mostra o impacto e pede confirmação. Revise o `down.sql`, pois nem toda alteração de enum é reversível sem reconstrução ou backup.
