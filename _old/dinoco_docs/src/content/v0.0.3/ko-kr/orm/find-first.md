# find_first

최대 하나의 레코드를 검색하는 데 사용됩니다.

---

## 수행할 수 있는 작업

- `.select::&lt;T&gt;()`: 기본 프로젝션을 사용자 정의 프로젝션으로 변경합니다.
- `.cond(...)`: 검색에 필터를 추가합니다.
- `.take(...)`: 고려되는 최대 레코드 수를 제한합니다.
- `.skip(...)`: 첫 번째 결과를 선택하기 전에 레코드를 건너뜠니다.
- `.order_by(...)`: 어떤 레코드를 먼저 고려해야 하는지 정의합니다.
- `.includes(...)`: 주요 항목과 함께 관계를 로드합니다.
- `.count(...)`: 프로젝션에서 관계 카운터를 계산합니다.
- `.cache(...)`: 먼저 Redis에서 검색을 시도하고 키가 존재하지 않는 경우에만 데이터베이스를 쿼리합니다. 캐시 적중 시 쿼리 로거는 `CACHE HIT key=...`를 기록합니다.
- `.cache_with_expiration(...)`: 동일한 흐름을 수행하지만 TTL을 초 단위로 저장합니다.
- `.read_in_primary()`: 주 데이터베이스에서 읽기를 강제합니다.
- `.execute(&client)`: 쿼리를 실행하고 최대 하나의 항목을 반환합니다.

## 반환

`select::&lt;T&gt;()`가 없으면 반환은 다음과 같습니다:

```rust
DinocoResult<Option<M>>
```

`select::&lt;T&gt;()`가 있으면 반환은 다음과 같이 변경됩니다:

```rust
DinocoResult<Option<T>>
```

## 기본 예시

```rust
let user = dinoco::find_first::<User>()
    .cond(|w| w.id.eq(10))
    .execute(&client)
    .await?;
```

## select를 사용한 예시

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserSummary {
    id: i64,
    name: String,
}

let user = dinoco::find_first::<User>()
    .select::<UserSummary>()
    .cond(|w| w.id.eq(1_i64))
    .execute(&client)
    .await?;
```

## 관계가 있는 예시

```rust
#[derive(Debug, Clone, dinoco::Extend)]
#[extend(User)]
struct UserWithPosts {
    id: i64,
    name: String,
    posts: Vec<Post>,
}

let user = dinoco::find_first::<User>()
    .select::<UserWithPosts>()
    .cond(|x| x.id.eq(1_i64))
    .includes(|x| x.posts())
    .execute(&client)
    .await?;
```

## 정렬을 사용한 예시

```rust
let latest_user = dinoco::find_first::<User>()
    .order_by(|x| x.id.desc())
    .execute(&client)
    .await?;
```

## 워커를 사용한 예시

```rust
use database::*;

let _worker = workers()
    .on::<User, _, _>("user.first-read", |job| async move {
        println!("첫 번째 사용자 읽음: {}", job.data.name);
        job.success();
    })
    .run()
    .await?;

let user = dinoco::find_first::<User>()
    .order_by(|x| x.id.desc())
    .enqueue("user.first-read")
    .execute(&client)
    .await?;
```

워커에 대한 자세한 내용은 [**`queues`**](/v0.0.2/orm/queues)를 참조하세요.

## 캐시를 사용한 예시

이 메서드는 스키마에 `redis`가 구성되어 있는 경우에만 존재합니다.

```rust
use database::*;

let user = dinoco::find_first::<User>()
    .cond(|x| x.id.eq(1_i64))
    .cache("users:1")
    .execute(&client)
    .await?;
```

## 캐시를 사용한 예시

이 메서드는 스키마에 `redis`가 구성되어 있는 경우에만 존재합니다.

```rust
use database::*;

let user = dinoco::find_first::<User>()
    .cond(|x| x.id.eq(1_i64))
    .cache_with_expiration("users:1")
    .execute(&client)
    .await?;
```

## 다음 단계

- [**`find_many::&lt;M&gt;()`**](/v0.0.2/orm/find-many): 여러 레코드를 검색합니다.
- [**`count::&lt;M&gt;()`**](/v0.0.2/orm/count): 레코드 수.
