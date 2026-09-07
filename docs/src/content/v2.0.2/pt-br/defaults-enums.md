# Defaults e enums

Um default pertence ao schema sempre que o valor é uma regra de banco, ou um identificador que a própria lib gerencia — não algo que a lógica da sua aplicação decide. Além de preencher o default da coluna no nível do banco, declarar `@default(...)` tem um segundo efeito que vale a pena saber cedo: ele remove esse field da lista de parâmetros do `new()` gerado, já que o Dinoco (ou o banco) já sabe como produzir um valor para ele.

## Defaults literais

Literais booleanos, numéricos, de string e de enum podem ser declarados diretamente:

```dinoco
model Feature {
    id      Integer @id @default(autoincrement())
    name    String
    enabled Boolean @default(false)
    weight  Float   @default(1.0)
}
```

O literal precisa ser compatível com o tipo do field — `@default(false)` em um `Boolean`, `@default(1.0)` em um `Float`, e assim por diante. Esse default é escrito na migration de verdade, não só reforçado no lado Rust, então linhas inseridas por qualquer coisa que não seja sua aplicação Dinoco (um `INSERT` manual, outro serviço) recebem o mesmo valor.

## Valores gerados

Quatro funções geradoras são suportadas, cada uma ligada a um tipo escalar específico:

- `autoincrement()` — só em fields `Integer`.
- `uuid()` — só em fields `String`.
- `snowflake()` — só em fields `Integer`.
- `now()` — só em fields `DateTime` ou `Date`.

> [!NOTE]
> O compiler verifica esse pareamento em tempo de compilação, não em tempo de migration. `id String @default(snowflake())` falha imediatamente com um erro claro, em vez de produzir um schema que só quebra quando você tenta rodar uma migration contra ele.

## UUID

```dinoco
id String @id @default(uuid())
```

O tipo Rust gerado é `dinoco::Uuid` — um tipo de identificador baseado em string que o pipeline de insert entende especificamente como um ID, não apenas qualquer `String`. O Dinoco gera o valor de verdade no lado do client, antes da linha ser inserida, o que é o que torna possível inserir um pai e suas linhas relacionadas (um-para-muitos, muitos-para-um, um-para-um) na mesma operação lógica: as linhas filhas podem referenciar a nova chave do pai antes mesmo da linha do pai chegar no banco.

## Snowflake

```dinoco
config {
    database          = "postgresql"
    database_url      = env("DATABASE_URL")
    snowflake_node_id = env("SNOWFLAKE_NODE_ID")
}

model Event {
    id Integer @id @default(snowflake())
}
```

O field Rust vira `dinoco::Snowflake`, baseado em `i64` — ordenável por horário de criação, ao contrário de um UUID aleatório, o que é o motivo dele costumar ser preferido em tabelas de alta escrita onde a ordem de inserção importa. O node ID é obrigatório a partir do momento em que qualquer field do schema usa `snowflake()`, e precisa vir do ambiente (veja [IDs Snowflake](/pt-br/docs/orm/guide/configuration#ids-snowflake)).

## Autoincremento

```dinoco
id Integer @id @default(autoincrement())
```

O próprio banco cria o inteiro — o Dinoco não o gera no lado do client como faz com UUIDs e Snowflakes. Quando um insert precisa retornar a entidade, ou propagar a nova chave para uma escrita de relação aninhada, o Dinoco recupera o valor gerado usando o mecanismo que o compiler SQL do adapter ativo emite (`RETURNING` no PostgreSQL e SQLite, uma leitura extra no MySQL).

## Enums

Declare um enum uma vez, depois use ele tanto como tipo de um field quanto dentro de um `@default(...)`:

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

Variantes Rust geradas são `PascalCase`, enquanto o valor de fato guardado no banco preserva a grafia exata do schema. O codegen emite um enum compacto, pronto para Serde, construído sobre o derive `DinocoEnum`:

```rust
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    dinoco::serde::Serialize,
    dinoco::serde::Deserialize,
    dinoco::DinocoEnum,
)]
#[serde(crate = "::dinoco::serde")]
pub enum Role {
    #[default]
    #[dinoco(value = "USER")]
    #[serde(rename = "USER")]
    User,

    #[dinoco(value = "ADMIN")]
    #[serde(rename = "ADMIN")]
    Admin,
}
```

O `DinocoEnum` gera toda conversão que `DinocoValue`, SQLite, PostgreSQL e MySQL precisam — não existe implementação por adapter para escrever à mão ou manter sincronizada.

Cada variante também ganha `#[serde(rename = "...")]`, então a (des)serialização JSON vai e volta pelo valor exato do banco, em vez do nome PascalCase da variante Rust. O mesmo mapeamento move o `Display`, então `.to_string()` também retorna a grafia do schema, e a direção inversa — `TryFrom<&str>`, `TryFrom<String>`, `FromStr` — retorna um erro para qualquer coisa que não seja um valor conhecido, em vez de dar panic. Concretamente: `waiting_payment` vira a variante `PaymentState::WaitingPayment`; `.to_string()` nela retorna `"waiting_payment"` de novo, e `PaymentState::try_from("waiting_payment")` reconstrói exatamente a mesma variante.

> [!TIP]
> Já tem um enum Rust que prefere escrever à mão em vez de gerar? Derive `DinocoEnum` diretamente nele e mapeie cada valor do banco explicitamente com `#[dinoco(value = "...")]`. Só variantes sem dados são suportadas (nada de `Variante(T)` ou `Variante { campo: T }`):
>
> ```rust
> #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, dinoco::DinocoEnum)]
> enum PaymentState {
>     #[default]
>     #[dinoco(value = "waiting-payment")]
>     Waiting,
>
>     #[dinoco(value = "paid")]
>     Paid,
> }
> ```

O armazenamento muda por adapter, mas a API do lado Rust não: PostgreSQL ganha um tipo enum nativo de verdade, MySQL usa a representação de enum do próprio dialeto, e SQLite — que não tem um conceito nativo de enum — guarda e valida uma representação escalar compatível.

## Alterando enums com segurança

`dinoco migrate generate` compara definições de enum da mesma forma que compara tabelas. Adicionar um valor novo costuma ser uma mudança puramente aditiva. Renomear ou remover um é diferente: linhas existentes ainda podem ter o valor antigo, então o planner de migration do Dinoco sinaliza isso como um risco destrutivo e pede confirmação antes de gerar um plano que possa invalidar dados.

> [!WARNING]
> Sempre leia o `up.sql` e o `down.sql` de uma mudança de enum antes de aplicá-la. Nem todo banco consegue reverter de forma limpa uma renomeação ou remoção de enum sem reconstruir uma coluna dependente, então o `down.sql` gerado merece o mesmo escrutínio que o `up.sql` — não assuma que o rollback é de graça só porque o Dinoco gerou um arquivo para ele.
