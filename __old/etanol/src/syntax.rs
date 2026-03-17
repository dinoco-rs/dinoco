// update unique
let user = User { ... };
user.name = "John";

user.update().execute();

// update many
User::update()
    .field("name", ModelWhere::Contains("name", "random"))
    .value("name", format!("John-{}", 2))
    .results() // retornar todos os modificados;
    .execute();