# insert_many

일괄 삽입에 사용됩니다.

---

## 수행할 수 있는 작업

- `.values(Vec&lt;M&gt;)`: 일괄 삽입될 레코드를 정의합니다.
- `.with_relation(Vec&lt;R&gt;)`: 각 부모에 대해 정확히 하나의 새 관련 항목을 삽입합니다.
- `.with_relations(Vec&lt;Vec\<R&gt;\>)` : 각 부모에 대해 여러 개의 새 관련 항목을 삽입합니다.
- `.with_connection(Vec&lt;R&gt;)`: 삽입된 각 부모에 대해 정확히 하나의 기존 관계를 연결합니다.
- `.with_connections(Vec&lt;Vec\<R&gt;\>)` : 삽입된 각 부모에 대해 여러 기존 관계를 연결합니다. 일반적으로 다대다 시나리오에서 사용됩니다.
- `.returning::&lt;T&gt;()`: 반환 값을 삽입된 항목의 유형화된 목록으로 변경합니다.
- `.execute(&client)`: 일괄 작업을 실행합니다.

## 반환 값

`.returning::&lt;T&gt;()`가 없으면 반환 값은 다음과 같습니다.

```rust
DinocoResult<()>
```

`.returning::&lt;T&gt;()`가 있으면 반환 값은 다음과 같습니다.

```rust
DinocoResult<Vec<T>>
```

## 간단한 예시

```rust
dinoco::insert_many::<User>()
    .values(vec![
        User { id: "u1".into(), email: "a@acme.com".into(), name: "A".into() },
        User { id: "u2".into(), email: "b@acme.com".into(), name: "B".into() },
    ])
    .execute(&client)
    .await?;
```

## 반환 값이 있는 예시

```rust
let created = dinoco::insert_many::<User>()
    .values(vec![
        User { id: 2, name: "Ana".to_string() },
        User { id: 3, name: "Caio".to_string() },
    ])
    .returning::<User>()
    .execute(&client)
    .await?;
```

## `with_relation(...)` 예시

각 부모가 생성되고 연결되어야 할 때 `with_relation(...)`을 사용합니다.

```rust
dinoco::insert_many::<Post>()
    .values(vec![
        Team { id: "team-11".into(), name: "Data".into() },
        Team { id: "team-12".into(), name: "DX".into() },
    ])
    .with_relation(vec![
        Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() },
        Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
    ])
    .execute(&client)
    .await?;
```

## `with_relations(...)` 예시

각 부모 레코드에 여러 자식이 있을 때 `with_relations(...)`를 사용합니다. 레코드는 자동으로 생성되고 연결됩니다.

```rust
dinoco::insert_many::<Post>()
    .values(vec![
        Team { id: "team-11".into(), name: "Data".into() },
        Team { id: "team-12".into(), name: "DX".into() },
    ])
    .with_relation(vec![
        vec![
			Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() },
        	Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
		],

		vec![Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() }]
    ])
    .execute(&client)
    .await?;
```

## `with_connection(...)` 예시

각 부모가 정확히 하나의 기존 레코드에 연결되어야 할 때 `with_connection(...)`을 사용합니다.

```rust
dinoco::insert_many::<Team>()
    .values(vec![
        Team { id: "team-11".into(), name: "Data".into() },
        Team { id: "team-12".into(), name: "DX".into() },
    ])
    .with_connection(vec![
        Member { id: "member-11".into(), name: "Rafa".into(), teamId: "legacy".into() },
        Member { id: "member-12".into(), name: "Bia".into(), teamId: "legacy".into() },
    ])
    .execute(&client)
    .await?;
```

## `with_connections(...)` 예시

각 부모가 여러 기존 레코드에 연결되어야 할 때 `with_connections(...)`를 사용합니다.
이는 다대다 관계에서 흔히 사용됩니다.

```rust
dinoco::insert_many::<Article>()
    .values(vec![
        Article { id: "article-11".into(), title: "Connect Multiple".into() },
        Article { id: "article-12".into(), title: "Connect Batch".into() },
    ])
    .with_connections(vec![
        vec![
            Label { id: "label-11".into(), name: "orm".into() },
            Label { id: "label-12".into(), name: "rust".into() },
        ],
        vec![
            Label { id: "label-10".into(), name: "backend".into() },
        ],
    ])
    .execute(&client)
    .await?;
```

## 참고 사항

- `with_relation(...)` 및 `with_relations(...)`는 부모가 이미 메모리에 알려진 ID를 가지고 있을 때 가장 잘 작동합니다.
- `with_connection(...)` 및 `with_connections(...)`는 기존 관계를 연결하는 데 중점을 둡니다.
- `with_connection(...)`은 `values(...)`와 동일한 수의 항목을 요구합니다.
- `with_connections(...)`는 주 벡터와 동일한 수의 그룹을 요구합니다.

## 다음 단계

- [**`insert_into::&lt;M&gt;()`**](/v0.0.1/orm/insert-into): 단일 삽입
- [**`update::&lt;M&gt;()`**](/v0.0.1/orm/update): 필터링된 업데이트
