# update

필터링된 레코드를 업데이트하는 데 사용됩니다.

---

## 수행 가능한 작업

- `.cond(...)`: 어떤 레코드를 업데이트할지 정의합니다.
- `.values(item)`: 레코드의 새 값을 지정합니다.
- `.connect(...)`: 쓰기 지원 관계 링크를 생성합니다.
- `.disconnect(...)`: 쓰기 지원 관계 링크를 제거합니다.
- `.returning::&lt;T&gt;()`: 업데이트된 레코드를 타입이 지정된 프로젝션으로 반환합니다.
- `.execute(&client)`: 데이터베이스에서 업데이트를 실행합니다.

## 반환

`.returning::&lt;T&gt;()`가 없으면 반환 값은 다음과 같습니다:

```rust
DinocoResult<()>
```

`.returning::&lt;T&gt;()`가 있으면 반환 값은 다음과 같습니다:

```rust
DinocoResult<Vec<T>>
```

참고:

- `update().returning()`은 `.connect(...)` 또는 `.disconnect(...)`를 사용한 관계 쓰기를 지원하지 않습니다.

## 필드 업데이트 예시

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .values(User {
        id: 10,
        email: "novo@acme.com".to_string(),
        name: "Novo Nome".to_string(),
    })
    .execute(&client)
    .await?;
```

## connect(...) 예시

쓰기 지원 관계(일반적으로 다대다 관계)를 연결하는 데 사용됩니다.

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .connect(|r| r.roles().slug.eq("admin"))
    .execute(&client)
    .await?;
```

## disconnect(...) 예시

관계를 끊는 데 사용됩니다.

```rust
dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .disconnect(|r| r.roles().slug.eq("guest"))
    .execute(&client)
    .await?;
```

## 워커 예시

```rust
use database::*;

let _worker = workers()
    .on::<User, _, _>("user.updated", |job| async move {
        println!("사용자 업데이트됨: {}", job.data.name);
        job.success();
    })
    .run()
    .await?;

dinoco::update::<User>()
    .cond(|w| w.id.eq(10))
    .values(User {
        id: 10,
        email: "novo@acme.com".to_string(),
        name: "Novo Nome".to_string(),
    })
    .enqueue("user.updated")
    .execute(&client)
    .await?;
```

워커에 대한 자세한 내용은 [**`queues`**](/v0.0.2/orm/queues)에서 확인하세요.

## connect 및 disconnect에서 사용 가능한 필터

- `eq`
- `neq`
- `gt`
- `gte`
- `lt`
- `lte`
- `in_values`
- `not_in_values`
- `is_null`
- `is_not_null`
- `includes`
- `starts_with`
- `ends_with`

## 다음 단계

- [**`update_many::&lt;M&gt;()`**](/v0.0.1/orm/update-many): 일괄 업데이트.
- [**`find_and_update::&lt;M&gt;()`**](/v0.0.1/orm/find-and-update): 반환이 있는 원자적 업데이트.
