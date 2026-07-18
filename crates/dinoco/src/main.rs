use dinoco::{Entity, EntityExtend, find_first};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, SqliteAdapter};

#[derive(Debug, Entity)]
#[dinoco(table_name = "user")]
pub struct User {
    #[dinoco(auto_generate = uuid)]
    id: String,
    email: String,
    password: String,
    office: String,

    #[dinoco(many_to_one, foreign_key = "userId", references = "id")]
    tokens: Vec<UserToken>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "usertoken")]
pub struct UserToken {
    #[dinoco(auto_generate = uuid)]
    id: String,

    #[dinoco(default = false)]
    isExpired: bool,

    userId: Option<String>,

    #[dinoco(one_to_many, foreign_key = "userId", references = "id")]
    user: Option<User>,
}

#[derive(Debug, Entity)]
#[dinoco(table_name = "user_post")]
pub struct UserPost {
    user_id: String,
    post_id: String,
}

#[derive(Debug, EntityExtend)]
#[extend(User)]
pub struct UserSelect {
    id: String,
    email: String,
}

#[tokio::main]
async fn main() {
    let adapter = SqliteAdapter::new("./database.sqlite".to_string()).await.unwrap();
    let backend = Backend::Sqlite(adapter);
    let client = DinocoClient::new(backend);

    let mut user = User::new("new@dinoco.rs".to_string(), "secret".to_string(), "BR".to_string());
    user.tokens = vec![UserToken::new()];
    let _insert = dinoco::insert_into::<User>().values(&user);

    let mut many_a = User::new("many-a@dinoco.rs".to_string(), "secret".to_string(), "BR".to_string());
    many_a.tokens = vec![UserToken::new()];
    let mut many_b = User::new("many-b@dinoco.rs".to_string(), "secret".to_string(), "BR".to_string());
    many_b.tokens = vec![UserToken::new()];
    let _insert_many = dinoco::insert_many::<User>().values(vec![many_a, many_b]);
    let _update = dinoco::update::<User>()
        .where_(|x| x.id.eq("user-id"))
        .update(|x| x.email.set("updated@dinoco.rs".to_string()));
    let _update_optional = dinoco::update_many::<UserToken>().update(|x| x.userId.set(Some("user-id".to_string())));
    let _update_null = dinoco::update::<UserToken>().update(|x| x.userId.set(None));
    let _connect =
        dinoco::update::<UserPost>().where_(|x| x.user_id.eq("user-id")).update(|x| x.post_id.connect("post-id"));
    let _disconnect = dinoco::update_many::<UserPost>()
        .where_(|x| x.user_id.batch(vec!["user-a", "user-b"]))
        .update(|x| x.post_id.disconnect("post-id"));
    let _find_and_update =
        dinoco::find_and_update::<User>().where_(|x| x.id.eq("user-id")).update(|x| x.email.set("atomic@dinoco.rs"));
    let _count = dinoco::count::<User>();
    let _count_with_tokens = dinoco::count::<User>().includes(|x| x.tokens().where_(|token| token.isExpired.eq(false)));
    let _delete = dinoco::delete::<UserToken>().where_(|x| x.id.eq("token-id"));
    let _delete_many = dinoco::delete_many::<UserToken>().where_(|x| x.isExpired.eq(true));

    let res = find_first::<User>()
        .select::<UserSelect>()
        .where_(|x| x.id.eq("252f168f-ac82-4657-8cf3-20ff377f45fb"))
        .read_in_primary()
        .execute(&client)
        .await
        .unwrap();

    println!("{res:#?}");

    let res = find_first::<UserToken>()
        .where_(|x| x.id.eq("5a9e84bd-c16e-43f1-b843-d7372ee50e2a"))
        .includes(|x| x.user())
        .execute(&client)
        .await
        .unwrap();

    println!("{res:#?}");
}
