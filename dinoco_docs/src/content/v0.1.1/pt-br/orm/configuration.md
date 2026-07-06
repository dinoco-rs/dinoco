# Configuração

O bloco `config {}` define como o Dinoco vai se conectar ao banco e quais recursos extras ficam disponíveis no projeto.

## O que entra no `config {}`

- `database`: define o adaptador principal, como `sqlite`, `postgresql` ou `mysql`.
- `database_url`: informa a URL de conexão principal.
- `read_replicas`: opcional, adiciona réplicas de leitura.

## Exemplo básico

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
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

- Com `read_replicas`, leituras podem usar réplica enquanto escritas continuam no banco principal.

## Quando editar esse bloco

- Ao trocar o banco principal do projeto.
- Ao configurar réplica de leitura.
- Ao ajustar variáveis de ambiente de conexão.

## Próximos passos

- [**Visão geral**](/v0.1.1/orm/supported-databases)
- [**Modelos**](/v0.1.1/orm/models)
- [**Relações**](/v0.1.1/orm/relations)
