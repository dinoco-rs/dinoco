# find_and_update

단일 레코드를 찾고, 데이터베이스에 원자적 업데이트를 적용하며, 업데이트된 항목을 반환하는 데 사용됩니다.

---

## 수행 가능한 작업

- `.cond(...)`
- `.update(...)`
- `.execute(&client)`

## 메서드 설명

- `.cond(...)`: 어떤 레코드를 찾을지 정의합니다.
- `.update(...)`: 모델의 필드에 원자적 작업을 적용합니다.
- `.execute(&client)`: 업데이트를 실행하고 업데이트된 레코드를 반환합니다.

## 반환 값

`find_and_update`의 반환 값은 다음과 같습니다:

```rust
DinocoResult<M>
```

## 기본 예시

```rust
let task = dinoco::find_and_update::<Task>()
    .cond(|x| x.id.eq(task_id.clone()))
    .update(|x| x.status.set(TaskStatus::REVIEW))
    .execute(&client)
    .await?;
```

## `ModelUpdate`에서 사용 가능한 작업

- `set(value)`
- `increment(value)`
- `decrement(value)`
- `multiply(value)`
- `division(value)`

## 참고 사항

- 업데이트는 단일 `UPDATE` 쿼리로 실행됩니다.
- 조건에 일치하는 행이 없으면 `DinocoError::RecordNotFound`가 반환됩니다.
- 업데이트 DSL은 관계를 노출하지 않습니다.
- 현재 이 흐름은 업데이트된 레코드를 찾고 반환하기 위해 단일 기본 키를 지원합니다.

## 다음 단계

- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): 전통적인 업데이트.
- [**`update_many::&lt;M&gt;()`**](/v0.0.1/orm/update-many): 일괄 업데이트.
