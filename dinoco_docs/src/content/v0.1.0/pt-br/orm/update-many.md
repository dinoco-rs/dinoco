# update_many

Atualiza todos os registros que correspondem ao filtro informado. Diferente de `update`, `.cond(...)` é opcional.

---

## O que você pode fazer

- `.cond(...)`: restringe os registros afetados; opcional.
- `.update(...)`: altera um campo em todos os registros selecionados; pode ser chamado várias vezes.
- `.returning()`: retorna os models atualizados.
- `.returning_as::<T>()`: retorna os registros atualizados em uma projeção tipada.
- `.execute(&client)`: executa uma única instrução `UPDATE`.

Sem `.cond(...)`, a atualização é aplicada a todos os registros do model.

## Atualizar registros filtrados

```rust
dinoco::update_many::<Device>()
    .cond(|device| device.platform.eq(DevicePlatform::Android))
    .update(|device| device.theme.set(DeviceTheme::Dark))
    .update(|device| device.token.set(None::<String>))
    .execute(&client)
    .await?;
```

## Atualizar todos os registros

```rust
dinoco::update_many::<User>()
    .update(|user| user.active.set(true))
    .execute(&client)
    .await?;
```

## Retorno tipado

```rust
let users = dinoco::update_many::<User>()
    .cond(|user| user.active.eq(false))
    .update(|user| user.active.set(true))
    .returning_as::<UserSummary>()
    .execute(&client)
    .await?;
```

## Próximos passos

- [**`update::<M>()`**](/v0.1.0/orm/update): atualização condicionada.
- [**`find_and_update::<M>()`**](/v0.1.0/orm/find-and-update): atualização atômica de um registro.
