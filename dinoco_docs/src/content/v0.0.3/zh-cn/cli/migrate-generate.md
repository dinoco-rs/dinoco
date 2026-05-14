# 如何使用？

`dinoco migrate generate` 命令根据当前模式生成迁移。

它将当前模式的状态与已知历史进行比较，并创建数据库演进所需的工件。

---

## 命令的作用

此命令会：

- 读取当前模式
- 生成新的本地迁移
- 准备 Dinoco 用于数据库演进的工件

可选地，它还可以立即应用迁移并生成 Rust 模型。

## 参数

### --apply

立即应用生成的迁移并生成 Rust 模型。

示例：

```bash
dinoco migrate generate --apply
```

## 不应用迁移的使用示例

```bash
dinoco migrate generate
```

当您希望时，此流程很有用：

- 在应用前检查迁移
- 审查版本控制中的更改
- 将生成和执行分离到不同阶段

## 立即应用迁移的使用示例

```bash
dinoco migrate generate --apply
```

当您希望时，此流程很有用：

- 快速更新本地数据库
- 在迁移后立即生成模型
- 在开发过程中更快地迭代

## 后续步骤

生成后，您可以：

```bash
dinoco migrate run
```

或者：

```bash
dinoco models generate
```
