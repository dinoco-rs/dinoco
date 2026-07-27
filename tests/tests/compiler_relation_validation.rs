use dinoco_compiler::compile;

#[test]
fn rejects_declarations_that_would_be_ambiguous_or_silently_miscompiled() {
    for (schema, expected) in [
        (
            r#"
            model User { id Integer @id }
            model User { id Integer @id }
            "#,
            "declared more than once",
        ),
        (
            r#"
            model User {
                id   Integer @id
                id   Integer
            }
            "#,
            "Field `User.id` is declared more than once",
        ),
        (r#"model User { id MissingType @id }"#, "unknown type"),
        (r#"model User { id Integer @id tags String[] }"#, "only model relation fields may be lists"),
        (
            r#"
            model User { id Integer @id posts Post?[] }
            model Post { id Integer @id users User[] }
            "#,
            "cannot be optional",
        ),
        (
            r#"
            enum Role { USER USER }
            model User { id Integer @id role Role }
            "#,
            "declared more than once",
        ),
        (
            r#"
            config { database = "sqlite" database = "mysql" }
            model User { id Integer @id }
            "#,
            "Config key `database` is declared more than once",
        ),
    ] {
        let error = compile(schema).expect_err(expected);
        assert!(error.message.contains(expected), "{error}");
    }
}

#[test]
fn rejects_relation_when_target_model_has_no_opposite_field() {
    let error = compile(
        r#"
        config {
            database          = "sqlite"
            database_url      = env("DATABASE_URL")
            snowflake_node_id = env("NODE_ID")
            read_replicas     = []
        }

        model Account {
            id         Integer @id @default(snowflake())
            name       String
            phone      String
            email      String
            password   String
            birth_date String
            document   String
            business   Business[]
        }

        model Business {
            id        Integer @id @default(snowflake())
            name      String
            document  String
            phone     String
            email     String
            invoicing String
        }

        model Address {
            id Integer @id @default(snowflake())
        }
        "#,
    )
    .expect_err("a one-sided relation must not compile");

    assert!(error.message.contains("Account.business"), "{error}");
    assert!(error.message.contains("Business"), "{error}");
    assert!(error.message.contains("opposite relation field"), "{error}");
}

#[test]
fn validates_the_opposite_requirement_from_either_model() {
    let error = compile(
        r#"
        model Account {
            id Integer @id
        }

        model Business {
            id      Integer @id
            account Account
        }
        "#,
    )
    .expect_err("the owning side also needs an opposite field");

    assert!(error.message.contains("Business.account"), "{error}");
    assert!(error.message.contains("Account"), "{error}");
}

#[test]
fn accepts_bidirectional_relation_with_fields_and_references() {
    compile(
        r#"
        model Account {
            id         Integer    @id
            businesses Business[]
        }

        model Business {
            id         Integer @id
            account_id Integer
            account    Account @relation(fields: [account_id], references: [id])
        }
        "#,
    )
    .expect("a complete one-to-many relation must compile");
}

#[test]
fn accepts_matching_named_relations_between_the_same_models() {
    compile(
        r#"
        model Account {
            id                Integer    @id
            owned_businesses  Business[] @relation(name: "BusinessOwner")
            audited_businesses Business[] @relation(name: "BusinessAuditor")
        }

        model Business {
            id         Integer @id
            owner_id   Integer
            auditor_id Integer
            owner      Account @relation(name: "BusinessOwner", fields: [owner_id], references: [id])
            auditor    Account @relation(name: "BusinessAuditor", fields: [auditor_id], references: [id])
        }
        "#,
    )
    .expect("matching names disambiguate multiple relations");
}

#[test]
fn rejects_mismatched_relation_names() {
    let error = compile(
        r#"
        model Account {
            id         Integer    @id
            businesses Business[] @relation(name: "OwnedBusinesses")
        }

        model Business {
            id         Integer @id
            account_id Integer
            account    Account @relation(name: "BusinessAccount", fields: [account_id], references: [id])
        }
        "#,
    )
    .expect_err("relation names must match on both sides");

    assert!(error.message.contains("OwnedBusinesses"), "{error}");
    assert!(error.message.contains("no compatible opposite"), "{error}");
}

#[test]
fn rejects_ambiguous_unnamed_relations() {
    let error = compile(
        r#"
        model Account {
            id         Integer    @id
            businesses Business[]
        }

        model Business {
            id         Integer @id
            owner_id   Integer
            auditor_id Integer
            owner      Account @relation(fields: [owner_id], references: [id])
            auditor    Account @relation(fields: [auditor_id], references: [id])
        }
        "#,
    )
    .expect_err("multiple relations require names");

    assert!(error.message.contains("Ambiguous relation"), "{error}");
    assert!(error.message.contains("Account.businesses"), "{error}");
}

#[test]
fn accepts_a_named_self_relation_with_two_distinct_sides() {
    compile(
        r#"
        model Employee {
            id         Integer    @id
            manager_id Integer?
            manager    Employee?  @relation(name: "Management", fields: [manager_id], references: [id])
            reports    Employee[] @relation(name: "Management")
        }
        "#,
    )
    .expect("a self relation with two matching sides must compile");
}

#[test]
fn rejects_a_self_relation_without_a_distinct_opposite_side() {
    let error = compile(
        r#"
        model Employee {
            id      Integer    @id
            reports Employee[] @relation(name: "Management")
        }
        "#,
    )
    .expect_err("a self relation cannot be its own opposite field");

    assert!(error.message.contains("Employee.reports"), "{error}");
    assert!(error.message.contains("no compatible opposite"), "{error}");
}

#[test]
fn rejects_snowflake_without_a_non_empty_node_id_env() {
    let missing = compile(
        r#"
        config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
        }

        model Event {
            id Integer @id @default(snowflake())
        }
        "#,
    )
    .expect_err("snowflake must require node configuration");
    assert!(missing.message.contains("snowflake_node_id"), "{missing}");

    let empty = compile(
        r#"
        config {
            database = "sqlite"
            database_url = env("DATABASE_URL")
            snowflake_node_id = env("")
        }

        model Event {
            id Integer @id @default(snowflake())
        }
        "#,
    )
    .expect_err("an empty environment-variable name is not usable");
    assert!(empty.message.contains("cannot be empty"), "{empty}");
}

#[test]
fn rejects_incomplete_empty_and_mismatched_relation_key_arrays() {
    let cases = [
        (
            "missing references",
            r#"
            model User {
                id    Integer @id
                posts Post[]
            }
            model Post {
                id      Integer @id
                user_id Integer
                user    User @relation(fields: [user_id])
            }
            "#,
            "together",
        ),
        (
            "empty arrays",
            r#"
            model User {
                id    Integer @id
                posts Post[]
            }
            model Post {
                id      Integer @id
                user_id Integer
                user    User @relation(fields: [], references: [])
            }
            "#,
            "cannot be empty",
        ),
        (
            "different lengths",
            r#"
            model User {
                id      Integer @id
                tenant  Integer @unique
                posts   Post[]
            }
            model Post {
                id        Integer @id
                user_id   Integer
                tenant_id Integer
                user      User @relation(fields: [user_id, tenant_id], references: [id])
            }
            "#,
            "same number",
        ),
    ];

    for (case, schema, expected) in cases {
        let error = compile(schema).expect_err(case);
        assert!(error.message.contains(expected), "{case}: {error}");
    }
}

#[test]
fn rejects_missing_non_scalar_mismatched_and_non_unique_relation_keys() {
    let cases = [
        (
            "missing local key",
            r#"
            model User { id Integer @id posts Post[] }
            model Post {
                id   Integer @id
                user User @relation(fields: [missing_id], references: [id])
            }
            "#,
            "missing local field",
        ),
        (
            "relation used as a local key",
            r#"
            model User { id Integer @id posts Post[] }
            model Post {
                id       Integer @id
                other_id Integer
                other    Other @relation(fields: [other_id], references: [id])
                user     User @relation(fields: [other], references: [id])
            }
            model Other { id Integer @id posts Post[] }
            "#,
            "must be a scalar or enum",
        ),
        (
            "key types differ",
            r#"
            model User { id String @id posts Post[] }
            model Post {
                id      Integer @id
                user_id Integer
                user    User @relation(fields: [user_id], references: [id])
            }
            "#,
            "incompatible key types",
        ),
        (
            "reference is not unique",
            r#"
            model User {
                id      Integer @id
                account Integer
                posts   Post[]
            }
            model Post {
                id         Integer @id
                account_id Integer
                user       User @relation(fields: [account_id], references: [account])
            }
            "#,
            "must declare @id or @unique",
        ),
    ];

    for (case, schema, expected) in cases {
        let error = compile(schema).expect_err(case);
        assert!(error.message.contains(expected), "{case}: {error}");
    }
}

#[test]
fn rejects_relation_optionality_and_unsafe_referential_actions() {
    let optionality = compile(
        r#"
        model User { id Integer @id posts Post[] }
        model Post {
            id      Integer @id
            user_id Integer?
            user    User @relation(fields: [user_id], references: [id])
        }
        "#,
    )
    .expect_err("nullable FK requires an optional relation");
    assert!(optionality.message.contains("optionality"), "{optionality}");

    let set_null = compile(
        r#"
        model User { id Integer @id posts Post[] }
        model Post {
            id      Integer @id
            user_id Integer
            user    User @relation(fields: [user_id], references: [id], onDelete: SetNull)
        }
        "#,
    )
    .expect_err("SetNull cannot target a required FK");
    assert!(set_null.message.contains("requires every local foreign-key field to be optional"), "{set_null}");

    let set_default = compile(
        r#"
        model User { id Integer @id posts Post[] }
        model Post {
            id      Integer @id
            user_id Integer
            user    User @relation(fields: [user_id], references: [id], onUpdate: SetDefault)
        }
        "#,
    )
    .expect_err("SetDefault requires a database default");
    assert!(set_default.message.contains("requires every local foreign-key field to define @default"), "{set_default}");
}

#[test]
fn rejects_one_to_many_without_an_owner_or_with_wrong_inverse_keys() {
    let missing_owner = compile(
        r#"
        model User {
            id    Integer @id
            posts Post[]
        }
        model Post {
            id   Integer @id
            user User
        }
        "#,
    )
    .expect_err("one-to-many needs a physical FK owner");
    assert!(missing_owner.message.contains("FK-owning side"), "{missing_owner}");

    let wrong_inverse = compile(
        r#"
        model User {
            id      Integer @id
            legacy  Integer @unique
            posts   Post[] @relation(fields: [legacy], references: [user_id])
        }
        model Post {
            id      Integer @id
            user_id Integer
            user    User @relation(fields: [user_id], references: [id])
        }
        "#,
    )
    .expect_err("explicit list keys must mirror the FK owner");
    assert!(wrong_inverse.message.contains("must mirror the owning side"), "{wrong_inverse}");
}

#[test]
fn validates_one_to_one_ownership_uniqueness_and_inverse_optionality() {
    compile(
        r#"
        model User {
            id      Integer @id
            profile Profile?
        }
        model Profile {
            id      Integer @id
            user_id Integer? @unique
            user    User? @relation(fields: [user_id], references: [id])
        }
        "#,
    )
    .expect("a unique FK on exactly one side is a valid one-to-one relation");

    let no_owner = compile(
        r#"
        model User { id Integer @id profile Profile? }
        model Profile { id Integer @id user User? }
        "#,
    )
    .expect_err("one-to-one needs exactly one owner");
    assert!(no_owner.message.contains("exactly one FK-owning side"), "{no_owner}");

    let no_unique = compile(
        r#"
        model User { id Integer @id profile Profile? }
        model Profile {
            id      Integer @id
            user_id Integer?
            user    User? @relation(fields: [user_id], references: [id])
        }
        "#,
    )
    .expect_err("one-to-one FK must be unique");
    assert!(no_unique.message.contains("requires @unique"), "{no_unique}");

    let required_inverse = compile(
        r#"
        model User { id Integer @id profile Profile }
        model Profile {
            id      Integer @id
            user_id Integer @unique
            user    User @relation(fields: [user_id], references: [id])
        }
        "#,
    )
    .expect_err("non-owning one-to-one side cannot be required");
    assert!(required_inverse.message.contains("non-owning side"), "{required_inverse}");

    let composite_relation_unique = compile(
        r#"
        model User {
            tenant Integer @unique
            id     Integer @unique
            detail Detail?
        }
        model Detail {
            id        Integer @id
            tenant_id Integer
            user_id   Integer
            user      User @unique @relation(fields: [tenant_id, user_id], references: [tenant, id])
        }
        "#,
    )
    .expect_err("a relation-level @unique cannot silently over-constrain a composite foreign key");
    assert!(composite_relation_unique.message.contains("Composite one-to-one"), "{composite_relation_unique}");
}

#[test]
fn validates_many_to_many_and_self_relation_shapes() {
    let mapped_many_to_many = compile(
        r#"
        model Post {
            id   Integer @id
            tags Tag[] @relation(fields: [id], references: [id])
        }
        model Tag {
            id    Integer @id
            posts Post[]
        }
        "#,
    )
    .expect_err("implicit many-to-many cannot carry direct key mappings");
    assert!(mapped_many_to_many.message.contains("cannot declare fields/references"), "{mapped_many_to_many}");

    let missing_id = compile(
        r#"
        model Post {
            title String
            tags  Tag[]
        }
        model Tag {
            id    Integer @id
            posts Post[]
        }
        "#,
    )
    .expect_err("implicit pivot generation needs deterministic primary keys");
    assert!(missing_id.message.contains("exactly one scalar @id"), "{missing_id}");

    let unnamed_self = compile(
        r#"
        model Employee {
            id      Integer @id
            manager Employee?
            reports Employee[]
        }
        "#,
    )
    .expect_err("self relation paths must be explicitly named");
    assert!(unnamed_self.message.contains("Self relation"), "{unnamed_self}");
}

#[test]
fn rejects_referential_actions_on_the_non_owning_side() {
    let error = compile(
        r#"
        model User {
            id    Integer @id
            posts Post[] @relation(onDelete: Cascade)
        }
        model Post {
            id      Integer @id
            user_id Integer
            user    User @relation(fields: [user_id], references: [id])
        }
        "#,
    )
    .expect_err("the list side has no database constraint to configure");

    assert!(error.message.contains("does not own a foreign key"), "{error}");
}
