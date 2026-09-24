# Fluxo das migrations manuais

Com `migration_engine = "manual"`, os comandos de migration mudam um pouco de significado. `dinoco models generate` se comporta igual nos dois motores.

## dinoco migrate generate

```bash
dinoco migrate generate create_users
```

Cria `dinoco/migrations/<timestamp>_create_users.rs`, registra em `dinoco/migrations/mod.rs` e regenera os models Rust. **Não** toca no banco, então não precisa de `DATABASE_URL`. Se você omitir o nome, a CLI pergunta. O nome é normalizado para `snake_case`; um nome sem letras ou dígitos é rejeitado.

Preencha `up` e `down` antes de rodar a migration. Uma migration recém-criada compila e não faz nada.

## dinoco migrate run

```bash
dinoco migrate run
```

Aplica todas as migrations pendentes, da mais antiga para a mais nova, e registra cada uma em `dinoco_manual_migrations` depois que o `up` tem sucesso. Rodar de novo sem pendências imprime `No pending migrations.` Este é o comando para o pipeline de deploy. Ele precisa da URL do banco.

## dinoco migrate rollback

```bash
dinoco migrate rollback            # reverte a última migration aplicada
dinoco migrate rollback --steps 3  # reverte as três últimas
```

Executa o `down` das migrations aplicadas mais novas, da mais nova para a mais antiga, e remove cada uma do histórico depois que o `down` tem sucesso. Pedir mais passos do que existem aplicados simplesmente para na mais antiga. `--steps 0` é rejeitado.

## dinoco migrate status

```bash
dinoco migrate status
```

```text
[applied] 20260919120000_create_users
[pending] 20260920093000_add_user_email
```

O `status` falha se o banco tem uma migration aplicada que não está no `mod.rs`. Isso significa que um arquivo foi apagado ou renomeado depois de aplicado.

Os quatro comandos aceitam `--workspace nome`/`-w nome`. `rollback` e `status` existem só para o motor manual: num schema `automatic` eles param com uma mensagem pedindo `migration_engine = "manual"`.

## Como as migrations são executadas

A CLI é um programa pré-compilado e não consegue carregar seus arquivos Rust. Para rodar uma migration ela gera um pequeno projeto Cargo em `dinoco/.runner/`, cujo `main.rs` inclui seu `dinoco/migrations/mod.rs`, e o executa com `cargo run`. As configurações do banco chegam por variáveis de ambiente definidas pela CLI, então a URL nunca aparece numa linha de comando.

Na prática:

- Uma toolchain Rust precisa estar disponível onde você roda `dinoco migrate run` — um job de CI que roda migrations precisa do `cargo`.
- A primeira execução compila o Dinoco em `dinoco/.runner/target/` e demora; as seguintes só recompilam o que mudou. O diretório se ignora sozinho (contém o próprio `.gitignore`), então nada precisa ser adicionado ao seu.
- O runner depende da mesma versão publicada do Dinoco que a CLI. As migrations podem usar `::dinoco` (e `std`), mas não os outros módulos ou crates da sua aplicação. Coloque essa lógica em SQL, ou rode o registro a partir do seu próprio binário, como na [referência do manager](/pt-br/docs/orm/guide/migration-manager#executando-um-registro-pelo-codigo).
- Um erro de compilação numa migration é impresso pelo Cargo, seguido de `the Dinoco runner failed`. Nenhuma migration roda quando o runner falha ao compilar.
- Você pode apagar `dinoco/.runner/` a qualquer momento; ele é recriado sob demanda.

## Workspaces

Com workspaces, cada workspace mantém as próprias migrations em `dinoco/migrations/<workspace>/` (com seu próprio `mod.rs`), e o `dinoco/mod.rs` gerado aponta `pub mod migrations;` para o selecionado:

```bash
dinoco migrate generate create_users --workspace dev
dinoco migrate run -w dev
```

## Fluxo recomendado

```bash
# Uma vez
dinoco init --migration-engine manual
dinoco migrate generate create_users
# edite dinoco/migrations/<timestamp>_create_users.rs: escreva `up` e `down`

dinoco migrate run          # aplica
dinoco migrate rollback     # testa o `down`
dinoco migrate run          # e aplica de novo

# No CI / deploy
dinoco migrate run
```

Mantenha o `schema.dinoco` alinhado com o que suas migrations constroem: os models são gerados a partir do schema, não das migrations, e no modo manual o Dinoco não compara os dois.

## Solução de problemas

| Mensagem | Causa |
| --- | --- |
| `dinoco/migrations/mod.rs was not found` | Nada foi criado ainda — rode `dinoco migrate generate <nome>` |
| `is missing the ... marker` | Os comentários marcadores do `mod.rs` foram removidos; restaure-os ou registre a migration à mão |
| `was applied but is not registered` | Uma migration aplicada foi apagada, renomeada ou tirada de `migrations()` |
| `is registered more than once` | O mesmo nome aparece duas vezes em `migrations()` |
| `migration ... failed while applying up` | Seu `up` retornou um erro; nada foi registrado, corrija e rode de novo |
| `failed to launch cargo` | Nenhuma toolchain Rust no `PATH` |
