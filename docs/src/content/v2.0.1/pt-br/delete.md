# Delete

O Dinoco separa deliberadamente a remoção em dois builders com garantias de segurança diferentes: o `delete` de propósito único, que exige um filtro em tempo de compilação, e o `delete_many` em massa, que permite uma operação intencionalmente sem filtro, na tabela inteira, quando é genuinamente isso que você quer.

## Remova um registro

```rust
dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .execute(&client)
    .await?;
```

Chamadas `.where_(...)` adicionais são permitidas e se combinam com `AND`, exatamente como qualquer outro builder.

## O filtro obrigatório

`delete::<M>()` começa a vida num type state que simplesmente não tem method `.execute()` nenhum — só chamar `.where_(...)` move ele para um estado onde `.execute()` existe. Isso significa que o código abaixo genuinamente não compila, não é só um erro em runtime:

```rust
// Inválido de propósito: delete exige where_.
dinoco::delete::<User>()
    .execute(&client)
    .await?;
```

> [!TIP]
> Esse truque de type state é o que previne o erro mais comum numa API de delete — esquecer o filtro e apagar uma tabela inteira — mantendo a API filtrada, o caso comum, exatamente tão concisa quanto seria sem essa rede de segurança.

## Remova vários registros

Use `delete_many` quando a operação é deliberadamente em massa:

```rust
dinoco::delete_many::<Session>()
    .where_(|x| x.expires_at.lt(cutoff))
    .execute(&client)
    .await?;
```

> [!WARNING]
> `delete_many::<M>().execute(...)` sem nenhum `.where_(...)` apaga toda linha da tabela, e compila normalmente — esse builder não tem a proteção de type state do `delete`. Isso é proposital, para que um job de reset completo de tabela genuíno consiga se expressar diretamente, mas significa que todo call site de `delete_many` sem filtro merece o mesmo escrutínio no review que um `DELETE FROM tabela` puro mereceria.

## Retorne os dados removidos

Os dois builders suportam uma projeção, retornando um vetor com as rows que de fato foram removidas:

```rust
let deleted = dinoco::delete::<User>()
    .where_(|x| x.id.eq(&user_id))
    .returning::<UserSummary>()
    .execute(&client)
    .await?;
```

Sem `.returning(...)`, a execução retorna `()` — não peça dados removidos quando um count, ou um log no nível da aplicação, já é tudo que você precisa; não tem motivo para pagar o custo de reconstruir rows que você está prestes a descartar.

## Relações e ações referenciais

O que acontece com rows relacionadas quando você apaga um parent vem inteiramente da ação referencial declarada no schema e imposta pela constraint de foreign key da migration — o runtime do Dinoco não adiciona uma camada própria de comportamento em cima disso:

- `Cascade` remove as rows dependentes junto do parent.
- `Restrict` ou `NoAction` podem rejeitar o delete de imediato enquanto existirem dependentes.
- `SetNull` desvincula rows relacionadas opcionais em vez de removê-las.
- `SetDefault` faz os dependentes recorrerem ao default declarado na foreign key.

O runtime nunca sobrescreve silenciosamente qualquer uma dessas escolhas do schema. Se um delete falhar por causa de `Restrict`, trate esse erro explicitamente — ou desconecte/reatribua a relação dependente você mesmo antes de tentar o delete — em vez de esperar que o Dinoco escolha um comportamento diferente por conta própria.
