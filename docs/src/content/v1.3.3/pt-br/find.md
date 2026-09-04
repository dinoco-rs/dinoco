# Visão geral de queries

Os builders de leitura do Dinoco são lazy: encadear methods descreve a consulta, enquanto `.execute(&client).await` compila o SQL no adapter e faz o I/O.

## Escolha o builder

| Objetivo | Builder | Retorno padrão |
| --- | --- | --- |
| Buscar zero ou uma row | `find_first::<M>()` | `Option<M>` |
| Buscar várias rows | `find_many::<M>()` | `Vec<M>` |
| Atualizar e retornar uma row | `find_and_update::<M>()` | `M` |
| Contar rows | `count::<M>()` | `MCount` |

Comece pela página específica:

- [Find first](/v1.3.3/orm/find-first)
- [Find many](/v1.3.3/orm/find-many)
- [Find and update](/v1.3.3/orm/find-and-update)
- [Count](/v1.3.3/orm/count)

## Etapas de uma query

Uma leitura normalmente segue quatro etapas:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true)) // 1. filtro
    .order_by(|account| account.id.asc())      // 2. ordem
    .take(25)                                  // 3. limite
    .execute(&client)                          // 4. execução
    .await?;
```

Somente `execute` acessa o banco.

## Recursos compartilhados

`find_first` e `find_many` compartilham:

- filtros tipados com `where_`;
- grupos booleanos com `where_complex`;
- busca `.fulltext(...)` em fields configurados;
- `select::<S>()`;
- `includes(...)`;
- `order_by(...)`;
- `read_in_primary()`.

`find_many` acrescenta `take` e `skip`.

## Próximos passos

Depois de escolher o builder, consulte:

- [Filtros](/v1.3.3/orm/filters) para operadores simples;
- [Where complex](/v1.3.3/orm/where-complex) para `AND`, `OR` e `NOT`;
- [Busca full-text](/v1.3.3/orm/full-text-search);
- [Select](/v1.3.3/orm/select);
- [Includes](/v1.3.3/orm/includes);
- [Transactions](/v1.3.3/orm/transactions).
