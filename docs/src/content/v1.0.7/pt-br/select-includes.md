# Select e includes

Select reduz as colunas escalares retornadas. Include preenche fields de relação. São operações diferentes e podem ser combinadas quando a projeção declara a forma necessária.

## Seleções customizadas

```rust
use dinoco::EntityExtend;

#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSummary {
    pub id: dinoco::Uuid,
    pub email: String,
}
```

```rust
let users = dinoco::find_many::<User>()
    .select::<UserSummary>()
    .order_by(|x| x.email.asc())
    .execute(&client)
    .await?;
```

O derive implementa projeção e conversão das rows nativas de todos os adapters.

## Inclua uma relação

```rust
let users = dinoco::find_many::<User>()
    .includes(|x| x.tokens())
    .execute(&client)
    .await?;

let token = dinoco::find_first::<UserToken>()
    .includes(|x| x.user())
    .execute(&client)
    .await?;
```

Sem include, relações permanecem no valor vazio gerado: `Vec::new()` ou `None`.

## Filtre um include

```rust
let users = dinoco::find_many::<User>()
    .includes(|x| {
        x.tokens()
            .where_(|token| token.is_expired.eq(false))
            .order_by(|token| token.id.desc())
            .take(5)
            .skip(0)
    })
    .execute(&client)
    .await?;
```

O `take(5)` vale por parent. O SQL usa window partition no data loader para limitar cada grupo, não o resultado global.

## Query complexa com includes aninhados

```rust
let projects = dinoco::find_many::<Project>()
    .where_(|project| project.archived.eq(false))
    .order_by(|project| project.created_at.desc())
    .includes(|project| project.owner())
    .includes(|project| {
        project
            .tasks()
            .where_(|task| task.priority.gte(5))
            .order_by(|task| task.priority.desc())
            .take(10)
            .includes(|task| task.assignee())
    })
    .take(25)
    .execute(&client)
    .await?;
```

Essa consulta limita 25 projects e, separadamente, dez tasks por project.

## Como as relações são carregadas

- Relações `one` usam left join.
- Relações `many` usam um data loader em batch com todas as parent keys.
- Includes irmãos rodam em paralelo.
- Includes aninhados repetem a estratégia no próximo nível.

A relation key viaja separada da projeção. Portanto, um `.select::<T>()` customizado não precisa expor a foreign key só para o loader agrupar children.

## Orientação prática

Use select quando o caller realmente ganha com um tipo menor. Inclua apenas relações necessárias e limite coleções potencialmente grandes. Para consistência após write, coloque `.read_in_primary()` no find principal; todos os includes seguem o mesmo backend.
