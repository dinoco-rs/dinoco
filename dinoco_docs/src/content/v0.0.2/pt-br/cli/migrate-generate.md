# Como usar?

O comando `dinoco migrate generate` gera uma migração a partir do esquema atual.

Ele compara o estado atual do esquema com o histórico conhecido e cria os artefatos necessários para evoluir o banco de dados.

---

## O que o comando faz

Este comando:

- Lê o esquema atual
- Gera uma nova migração local
- Prepara os artefatos usados pelo Dinoco para a evolução do banco de dados

Opcionalmente, ele também pode aplicar a migração imediatamente e gerar os modelos Rust.

## Parâmetros

### --apply

Aplica a migração gerada imediatamente e também gera os modelos Rust.

Exemplo:

```bash
dinoco migrate generate --apply
```

## Exemplo de uso sem aplicar

```bash
dinoco migrate generate
```

Este fluxo é útil quando você deseja:

- Inspecionar a migração antes de aplicar
- Revisar as mudanças no controle de versão
- Separar a geração e a execução em diferentes etapas

## Exemplo de uso com aplicação imediata

```bash
dinoco migrate generate --apply
```

Este fluxo é útil quando você deseja:

- Atualizar o banco de dados local rapidamente
- Gerar os modelos logo após a migração
- Iterar mais rápido durante o desenvolvimento

## Próximos passos

Após a geração, você pode:

```bash
dinoco migrate run
```

ou:

```bash
dinoco models generate
```
