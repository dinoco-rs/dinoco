# 구성

`config {}` 블록은 Dinoco가 데이터베이스에 연결하는 방법과 프로젝트에서 어떤 추가 기능을 사용할 수 있는지 정의합니다.

## `config {}`에 포함되는 내용

- `database`: `sqlite`, `postgresql` 또는 `mysql`과 같은 기본 어댑터를 정의합니다.
- `database_url`: 기본 연결 URL을 지정합니다.
- `read_replicas`: 선택 사항이며, 읽기 복제본을 추가합니다.
- `redis`: 선택 사항이며, Redis 기반 캐시 및 큐를 활성화합니다.

## 기본 예시

```dinoco
config {
    database = "sqlite"
    database_url = env("DATABASE_URL")
}
```

## Redis 예시

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")

    redis = {
        url = env("REDIS_URL")
    }
}
```

## 복제본 예시

```dinoco
config {
    database = "postgresql"
    database_url = env("DATABASE_URL")

    read_replicas = [
        env("DATABASE_READ_URL_1"),
        env("DATABASE_READ_URL_2")
    ]
}
```

## 생성된 API의 변경 사항

- `redis`를 사용하면 프로젝트는 `client.cache()`, `.cache(...)`와 같은 보조 함수와 `workers()`를 사용한 큐를 노출합니다.
- `read_replicas`를 사용하면 읽기는 복제본을 사용할 수 있고 쓰기는 기본 데이터베이스에서 계속됩니다.
- `redis`가 없으면 캐시 및 큐 기능이 완전히 제공되지 않습니다.

## 이 블록을 편집할 때

- 프로젝트의 기본 데이터베이스를 변경할 때.
- 캐시 또는 큐를 위해 Redis를 추가할 때.
- 읽기 복제본을 구성할 때.
- 연결 환경 변수를 조정할 때.

## 다음 단계

- [**개요**](/v0.0.2/orm/supported-databases)
- [**모델**](/v0.0.2/orm/models)
- [**관계**](/v0.0.2/orm/relations)
