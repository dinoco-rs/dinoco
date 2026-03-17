use etanol::{ModelWhere, QueryValue};

mod database;

use database::{create_connection, user::User};

fn main() {
    create_connection();

    // Insert
    let user = User {
        id: "5".to_string(),
        age: Some(5),
        name: "Teste".to_string(),
        ..User::default()
    };

    user.insert().execute();

    // FindOne
    let _user = User::find()
        .field(ModelWhere::Equal("name", "Random"))
        .load()
        .unwrap();

    // FindMany
    let _users = User::find()
        .field(ModelWhere::Equal("name", "Random"))
        .many()
        .load()
        .unwrap();

    // Update
    User::update()
        .field(ModelWhere::Equal("id", "1"))
        .value(QueryValue("name", "Matheus"))
        .execute()
        .unwrap();

    // delete
    User::delete().field(ModelWhere::Equal("id", "1")).execute();
}
