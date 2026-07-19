# 모델

`모델`은 Dinoco 스키마에서 애플리케이션의 핵심 엔티티를 정의합니다. 각 `모델`은 일반적으로 데이터베이스의 테이블을 나타내며 코드 생성, 타입 지정 쿼리 및 Dinoco API와의 작업의 기반이 됩니다.

---

## 모델이 나타내는 것

`모델`은 다음을 설명합니다:

- 엔티티의 이름.
- 데이터베이스에 저장된 필드.
- 어떤 필드가 필수 또는 선택 사항인지.
- 어떤 필드가 고유하거나 식별자인지.
- 이러한 데이터가 코드 생성 및 API에 의해 어떻게 사용될 것인지.

예시:

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique
	name  String?
}
```

이 예시에서:

- `User`는 모델입니다.
- `id`, `email`, `name`은 스칼라 필드입니다.
- `id`는 기본 식별자입니다.
- `email`은 고유성 제약 조건이 있습니다.

## 전체 예시

모델이 있는 간단한 스키마는 일반적으로 다음과 같습니다:

```dinoco
config {
	database = "postgresql"
	database_url = env("DATABASE_URL")
}

model User {
	id        Integer  @id @default(autoincrement())
	email     String   @unique
	name      String?
	active    Boolean  @default(true)
	createdAt DateTime @default(now())
}

model Post {
	id        Integer  @id @default(autoincrement())
	title     String
	content   String?
	published Boolean  @default(false)
	createdAt DateTime @default(now())
}
```

## 필드의 구조

모델의 각 필드는 다음으로 구성됩니다:

- 이름
- 타입
- 선택적 수정자
- 선택적 속성

예시:

```dinoco
email String @unique
```

이 줄에서:

- `email`은 필드 이름입니다.
- `String`은 타입입니다.
- `@unique`는 속성입니다.

## 필드 타입

필드는 텍스트, 숫자, 부울 및 날짜와 같은 스키마의 기본 값을 나타낼 수 있습니다.

### 스칼라 필드

텍스트, 숫자, 부울 및 날짜와 같은 직접적인 값을 저장하는 필드입니다.

```dinoco
model Product {
	id          Integer  @id @default(autoincrement())
	name        String
	description String?
	price       Float
	active      Boolean  @default(true)
	createdAt   DateTime @default(now())
}
```

## 타입 수정자

Dinoco는 두 가지 주요 수정자를 지원합니다:

| 수정자 | 의미       | 예시          |
| :---------- | :------------- | :-------------- |
| `?`         | 선택적 필드 | `name String?`  |
| `[]`        | 목록          | `tags String[]` |

### 선택적 필드

```dinoco
model User {
	id   Integer @id @default(autoincrement())
	name String?
}
```

`name`은 데이터베이스 및 생성된 계층에 따라 null이거나 없을 수 있습니다.

### 목록 필드

```dinoco
model Article {
	id   Integer  @id @default(autoincrement())
	tags String[]
}
```

이 형식은 데이터베이스와 흐름이 이러한 유형의 구조를 지원할 때 값 목록을 나타냅니다.

## 가장 일반적인 속성

속성은 필드 및 모델의 동작을 변경합니다.

| 속성        | 사용                              |
| :-------------- | :------------------------------- |
| `@id`           | 기본 식별자 정의 |
| `@default(...)` | 기본값 정의           |
| `@unique`       | 고유성 보장                |

### `@id`

레코드를 고유하게 식별하는 필드를 정의합니다.

```dinoco
id Integer @id @default(autoincrement())
```

생성된 API가 안전하게 작동하려면 모든 모델에 명확한 식별자가 있어야 합니다.

### `@default(...)`

필드의 기본값을 정의합니다.

```dinoco
active    Boolean  @default(true)
createdAt DateTime @default(now())
id        Integer  @default(autoincrement())
```

일반적인 함수 및 값:

| 예시                     | 사용                 |
| :-------------------------- | :------------------ |
| `@default(false)`           | 기본 부울     |
| `@default(now())`           | 현재 날짜          |
| `@default(autoincrement())` | 증분 정수 |
| `@default(uuid())`          | UUID 식별자  |

### `@unique`

필드 값이 반복되지 않도록 보장합니다.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique
}
```

이 속성은 이메일, 사용자 이름 및 외부 코드와 같은 필드에 이상적입니다.

## 모델 데코레이터

개별 필드의 속성 외에도 Dinoco는 전체 모델 블록에 적용되는 데코레이터도 지원합니다.

| 데코레이터             | 사용                                   |
| :-------------------- | :------------------------------------ |
| `@@ids([...])`        | 복합 기본 키 정의        |
| `@@table_name("...")` | 데이터베이스의 실제 테이블 이름 매핑 |

### `@@ids([...])`

레코드의 ID가 둘 이상의 필드에 의존하는 경우 `@@ids`를 사용합니다.

```dinoco
model Membership {
	userId Integer
	teamId Integer
	role   String

	@@ids([userId, teamId])
}
```

이 형식은 연관 테이블 및 자연적인 고유성이 이미 복합적인 시나리오에서 유용합니다.

### `@@table_name("...")`

스키마에서 더 친숙한 모델 이름을 유지하면서 데이터베이스의 다른 물리적 이름에 매핑하려는 경우 `@@table_name()`을 사용합니다.

```dinoco
model User {
	id    Integer @id @default(autoincrement())
	email String  @unique

	@@table_name("users")
}
```

이 경우:

- 모델은 스키마와 생성된 API에서 `User`로 계속 불립니다.
- 데이터베이스의 물리적 테이블은 `users`가 됩니다.

## 사용자 모델 예시

```dinoco
model User {
	id        Integer  @id @default(autoincrement())
	email     String   @unique
	name      String?
	active    Boolean  @default(true)
	createdAt DateTime @default(now())
}
```

코드 생성 후 이 모델은 Dinoco API와 직접 사용할 수 있습니다.

## Dinoco API를 사용한 사용자 검색 예시

### 단일 레코드 검색

```rust
let user = dinoco::find_first::<User>()
    .cond(|x| x.id.eq(1_i64))
    .execute(&client)
    .await?;
```

### 여러 레코드 검색

```rust
let users = dinoco::find_many::<User>()
    .cond(|x| x.name.includes("Ana"))
    .order_by(|x| x.id.asc())
    .take(10)
    .execute(&client)
    .await?;
```

## Dinoco API를 사용한 사용자 생성 예시

```rust
dinoco::insert_into::<User>()
    .values(User {
        id: 0,
        email: "bia@dinoco.rs".to_string(),
        name: Some("Bia".to_string()),
        active: true,
        createdAt: dinoco::Utc::now(),
    })
    .execute(&client)
    .await?;
```

## Dinoco API를 사용한 사용자 업데이트 예시

```rust
dinoco::update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .values(User {
        id: 1,
        email: "bia@dinoco.rs".to_string(),
        name: Some("Beatriz".to_string()),
        active: true,
        createdAt: dinoco::Utc::now(),
    })
    .execute(&client)
    .await?;
```

단일 필드에 대한 원자적 업데이트를 원한다면 `find_and_update` 흐름이 훨씬 더 직접적입니다:

```rust
let user = dinoco::find_and_update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .update(|x| x.name.set("Beatriz"))
    .execute(&client)
    .await?;
```

## Dinoco API를 사용한 사용자 제거 예시

```rust
dinoco::delete::<User>()
    .cond(|x| x.id.eq(1_i64))
    .execute(&client)
    .await?;
```

일괄 제거의 경우:

```rust
dinoco::delete_many::<User>()
    .cond(|x| x.active.eq(false))
    .execute(&client)
    .await?;
```

## 빠른 요약

| 개념       | 예시                | 목표                    |
| :------------- | :--------------------- | :-------------------------- |
| 모델          | `model User { ... }`   | 엔티티 표현    |
| 스칼라 필드  | `email String`         | 단순 값 저장     |
| 선택적 필드 | `name String?`         | 값 부재 허용  |
| 목록 필드 | `tags String[]`        | 여러 값 저장 |
| ID             | `id Integer @id`       | 고유하게 식별      |
| 기본값        | `@default(now())`      | 자동 채우기   |
| 고유         | `email String @unique` | 중복 방지          |

## 새 모델을 생성하는 시점

다음과 같은 경우 일반적으로 새 `모델`을 생성합니다:

- 애플리케이션의 엔티티가 데이터베이스에 영구적으로 저장되어야 할 때.
- 자체 ID를 가져야 할 때.
- 독립적으로 쿼리되어야 할 때.
- 자체 읽기 및 쓰기 규칙을 가져야 할 때.

일반적인 예시:

- `User`
- `Post`
- `Comment`
- `Category`
- `Order`
- `Invoice`
