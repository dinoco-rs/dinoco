# Enums

Enums permitem restringir um campo a um conjunto fixo de valores conhecidos no schema do Dinoco.

Eles são úteis quando o valor precisa ser previsível, validado e reaproveitado entre models.

---

## O que é um enum

Um `enum` define uma lista fechada de valores possíveis.

```dinoco
enum Role {
	USER
	ADMIN
}
```

Nesse caso, `Role` só pode assumir `USER` ou `ADMIN`.

## Uso em models

Depois de definido, o enum pode ser usado como tipo de campo em qualquer model.

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

Aqui:

- `role` usa o enum `Role`.
- `@default(USER)` define o valor padrão do campo.

## Defaults em enums

O valor informado em `@default(...)` é o valor usado no model gerado.

```dinoco
enum UserRule {
	ADMIN
	USER
	MEMBER
}

model User {
	id   Integer  @id @default(autoincrement())
	rule UserRule @default(MEMBER)
}
```

Nesse caso, o default gerado é `UserRule::MEMBER`.

O compiler também aceita o valor do default sem depender de caixa alta ou baixa, desde que exista um valor equivalente no enum:

```dinoco
rule UserRule @default(member)
```

Esse exemplo também será normalizado para `MEMBER`.

## Quando usar enums

Enums são úteis para representar valores como:

- Papéis de usuário
- Status de publicação
- Etapas de workflow
- Situações de pagamento

Exemplo:

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

## Boas práticas

- Use enums quando os valores possíveis forem conhecidos e finitos.
- Prefira nomes em PascalCase para o enum e valores em UPPER_CASE.
- Use `@default(...)` quando houver um estado inicial natural.

## Próximos passos

- [**Relações**](/v0.1.0/orm/relations): veja `@relation`, `onDelete`, `onUpdate` e tipos de relacionamento.
- [**Models**](/v0.1.0/orm/models): veja onde enums entram na definição de campos e no schema principal.
