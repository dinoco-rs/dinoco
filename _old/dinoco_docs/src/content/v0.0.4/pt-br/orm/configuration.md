# Configuração

O bloco `config {}` define como o Dinoco vai se conectar ao banco e quais recursos extras ficam disponíveis no projeto.

## O que entra no `config {}`

- `database`: define o adaptador principal, como `sqlite`, `postgresql` ou `mysql`.
- `database_url`: informa a URL de conexão principal.
- `read_replicas`: opcional, adiciona réplicas de leitura.
- `redis`: opcional, habilita cache e filas baseadas em Redis.

## Exemplo básico

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}
```

## Exemplo com Redis

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")

    redis = {
        url = env("REDIS_URL")
    }
}
```

## Exemplo com réplicas

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")

    read_replicas = [
        env("DATABASE_READ_URL_1"),
        env("DATABASE_READ_URL_2")
    ]
}
```

## O que muda na API gerada

- Com `redis`, o projeto passa a expor funções auxiliares como `client.cache()`, `.cache(...)` e filas com `workers()`.
- Com `read_replicas`, leituras podem usar réplica enquanto escritas continuam no banco principal.
- Sem `redis`, recursos de cache e fila não ficam totalmente disponíveis.

## Quando editar esse bloco

- Ao trocar o banco principal do projeto.
- Ao adicionar Redis para cache ou filas.
- Ao configurar réplica de leitura.
- Ao ajustar variáveis de ambiente de conexão.

## Próximos passos

- [**Visão geral**](/v0.0.2/orm/supported-databases)
- [**Modelos**](/v0.0.2/orm/models)
- [**Relações**](/v0.0.2/orm/relations)
