use dinoco::{Entity, EntityExtend, find_first};
use dinoco_engine::{Backend, DinocoAdapter, DinocoClient, SqliteAdapter};

#[derive(Debug, Entity)]
#[dinoco(table_name = "user")]
pub struct User {
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
    id: String,
    isExpired: bool,

    userId: Option<String>,

    #[dinoco(one_to_many, foreign_key = "userId", references = "id")]
    user: Option<User>,
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
    let client = DinocoClient { backend };

    let res = find_first::<User>()
        .select::<UserSelect>()
        .where_(|x| x.id.eq("252f168f-ac82-4657-8cf3-20ff377f45fb"))
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
