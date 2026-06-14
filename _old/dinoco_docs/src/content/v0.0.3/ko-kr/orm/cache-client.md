# 캐시

`client.cache()`는 `find_first` 및 `find_many`에 내장된 캐시 헬퍼에 의존하지 않고 `DinocoClient`에 구성된 Redis에 직접 액세스할 수 있도록 합니다.

## 무엇을 할 수 있나요

- `.get::&lt;T&gt;(...)`로 키 읽기
- `.set(...)`로 키 저장
- `.set_with_ttl(...)`을 사용하여 만료와 함께 저장
- `.delete(...)`로 키 제거

## 언제 사용하나요

다음과 같은 경우 `client.cache()`를 사용하세요:

- 수동 캐시 설정
- 쓰기 작업 후 키 무효화
- 여러 쿼리 간에 페이로드 공유
- 빠른 읽기를 위해 준비된 구조 저장

## 작동 방식

이 메서드는 `DinocoClientConfig::with_redis(...)`에 구성된 Redis를 사용합니다.

클라이언트에 Redis가 구성되어 있지 않으면 작업이 오류를 반환합니다.

## 사용 가능한 메서드

- `.get::&lt;T&gt;(key)`: 값을 `T`로 가져와 역직렬화합니다.
- `.set(key, &value)`: TTL 없이 직렬화하고 저장합니다.
- `.set_with_ttl(key, &value, ttl_seconds)`: 초 단위 만료와 함께 직렬화하고 저장합니다.
- `.delete(key)`: 키를 제거합니다.

## 기본 예제

```rust
use database::*;

let cache = client.cache();

cache.set("users:count", &42_i64).await?;

let count = cache.get::<i64>("users:count").await?;

println!("{count:?}");
```

## 타입이 지정된 목록 예제

```rust
use database::*;

let users = vec![
    User { id: 1, name: "Matheus".to_string() },
    User { id: 2, name: "Ana".to_string() },
];

client.cache().set("users:list", &users).await?;

let cached = client.cache().get::<Vec<User>>("users:list").await?;
```

## TTL 예제

```rust
use database::*;

client.cache().set_with_ttl("users:top-10", &vec![1, 2, 3], 60).await?;
```

## 무효화 예제

```rust
use database::*;

dinoco::update::<User>()
    .cond(|x| x.id.eq(1_i64))
    .values(User { id: 1, name: "새 이름".to_string() })
    .execute(&client)
    .await?;

client.cache().delete("users:1").await?;
client.cache().delete("users:list").await?;
```

## 지원되는 타입

값은 JSON으로 직렬화되므로 타입은 `serde`와 호환되어야 합니다.

일반적인 예시:

- `Vec&lt;User&gt;`
- `Option&lt;User&gt;`
- `String`
- `bool`
- `i64`
- 직렬화 가능한 구조체

## 참고 사항

- `client.cache()`는 수동 캐시입니다. 데이터베이스 쿼리를 실행하지 않습니다.
- 쿼리에 통합된 캐시의 경우 `find_first().cache(...)` 및 `find_many().cache(...)`를 사용하세요.
- `client.cache()`는 원하는 만큼 호출할 수 있습니다. 가벼운 래퍼만 생성합니다.

## 다음 단계

- [**`find_first::&lt;M&gt;()`**](/v0.0.2/orm/find-first)
- [**`find_many::&lt;M&gt;()`**](/v0.0.2/orm/find-many)
- [**`queues`**](/v0.0.2/orm/queues)
