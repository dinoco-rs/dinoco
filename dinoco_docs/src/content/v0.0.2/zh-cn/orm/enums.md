# 枚举

枚举允许将字段限制为 Dinoco schema 中一组固定的已知值。

当值需要可预测、经过验证并在模型之间重用时，它们非常有用。

---

## 什么是枚举

一个 `enum` 定义了一个封闭的可能值列表。

```dinoco
enum Role {
	USER
	ADMIN
}
```

在这种情况下，`Role` 只能是 `USER` 或 `ADMIN`。

## 在模型中使用

定义后，枚举可以在任何模型中用作字段类型。

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

这里：

- `role` 使用 `Role` 枚举。
- `@default(USER)` 定义了字段的默认值。

## 何时使用枚举

枚举对于表示以下值很有用，例如：

- 用户角色
- 发布状态
- 工作流阶段
- 支付情况

示例：

```dinoco
enum PostStatus {
	DRAFT
	REVIEW
	PUBLISHED
	ARCHIVED
}

model Post {
	id     Integer    @id @default(autoincrement())
	title  String
	status PostStatus @default(DRAFT)
}
```

## 最佳实践

- 当可能的值已知且有限时，使用枚举。
- 枚举名称首选 PascalCase，值首选 UPPER_CASE。
- 当存在自然的初始状态时，使用 `@default(...)`。

## 下一步

- [**关系**](/v0.0.1/orm/relations)：查看 `@relation`、`onDelete`、`onUpdate` 和关系类型。
- [**模型**](/v0.0.1/orm/models)：查看枚举在字段定义和主 schema 中的作用。
