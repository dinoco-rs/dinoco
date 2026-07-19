# 列挙型

列挙型を使用すると、Dinocoスキーマ内の既知の固定値のセットにフィールドを制限できます。

値が予測可能で、検証され、モデル間で再利用される必要がある場合に役立ちます。

---

## 列挙型とは

`enum` は、可能な値の閉じたリストを定義します。

```dinoco
enum Role {
	USER
	ADMIN
}
```

この場合、`Role` は `USER` または `ADMIN` のいずれかのみを取ることができます。

## モデルでの使用

定義されると、列挙型は任意のモデルのフィールド型として使用できます。

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

ここでの説明:

- `role` は `Role` 列挙型を使用します。
- `@default(USER)` はフィールドのデフォルト値を定義します。

## 列挙型を使用するタイミング

列挙型は、次のような値を表現するのに役立ちます。

- ユーザーの役割
- 公開ステータス
- ワークフローの段階
- 支払い状況

例:

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

## ベストプラクティス

- 可能な値が既知で有限である場合は、列挙型を使用します。
- 列挙型にはPascalCase、値にはUPPER_CASEの名前を推奨します。
- 自然な初期状態がある場合は、`@default(...)` を使用します。

## 次のステップ

- [**リレーション**](/v0.0.1/orm/relations): `@relation`、`onDelete`、`onUpdate`、およびリレーションシップの種類を参照してください。
- [**モデル**](/v0.0.1/orm/models): 列挙型がフィールド定義とメインスキーマにどのように組み込まれるかを参照してください。
