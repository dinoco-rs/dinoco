# Visão geral de queries

Todo builder de leitura do Dinoco é lazy: encadear `.where_(...)`, `.order_by(...)`, `.take(...)` e o resto só *descreve* a query. Nada toca o banco até `.execute(&client).await` rodar — que compila a descrição no SQL do adapter ativo e faz o I/O de fato.

## Escolha o builder

| Objetivo | Builder | Retorno padrão |
| --- | --- | --- |
| Buscar zero ou uma row | `find_first::<M>()` | `Option<M>` |
| Buscar várias rows | `find_many::<M>()` | `Vec<M>` |
| Atualizar e retornar uma row | `find_and_update::<M>()` | `M` |
| Contar rows | `count::<M>()` | `MCount` |

Vá direto para a página do builder que você precisa:

- [Find first](/pt-br/docs/orm/orm/find-first)
- [Find many](/pt-br/docs/orm/orm/find-many)
- [Find and update](/pt-br/docs/orm/orm/find-and-update)
- [Count](/pt-br/docs/orm/orm/count)

## Etapas de uma query

A maioria das leituras segue as mesmas quatro etapas, na mesma ordem:

```rust
let accounts = dinoco::find_many::<Account>()
    .where_(|account| account.active.eq(true)) // 1. filtro
    .order_by(|account| account.id.asc())      // 2. ordem
    .take(25)                                  // 3. limite
    .execute(&client)                          // 4. execução
    .await?;
```

> [!NOTE]
> Só a etapa 4, `.execute(...)`, chega de fato ao banco. Tudo antes dela é só construir um valor na memória — você pode atribuir uma query parcialmente construída a uma variável, passá-la para uma função e continuar encadeando nela depois, tudo isso sem um único round-trip de rede acontecer.

## Recursos compartilhados

`find_first` e `find_many` compartilham os mesmos blocos de construção:

- filtros tipados com `where_`;
- grupos booleanos com `where_complex`;
- `.fulltext(...)` em fields configurados para isso;
- `select::<S>()`;
- `includes(...)`;
- `order_by(...)`;
- `read_in_primary()`.

`find_many` acrescenta `take` e `skip`, já que paginação só faz sentido quando mais de uma row pode voltar.

## Próximos passos

Depois de escolher um builder, estas páginas cobrem as peças que se encaixam nele:

- [Filtros](/pt-br/docs/orm/orm/filters) para os operadores simples (`eq`, `gt`, `like`, e por aí vai).
- [Where complex](/pt-br/docs/orm/orm/where-complex) para agrupamento explícito de `AND`, `OR` e `NOT`.
- [Busca full-text](/pt-br/docs/orm/orm/full-text-search).
- [Select](/pt-br/docs/orm/orm/select) para projeções tipadas.
- [Includes](/pt-br/docs/orm/orm/includes) para carregar relações.
- [Transactions](/pt-br/docs/orm/orm/transactions) para agrupar leituras e escritas atomicamente.
