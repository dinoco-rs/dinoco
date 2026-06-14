#[path = "../common.rs"]
mod common;

use dinoco::insert_into;

use common::{Team, User};

fn main() {
    let _ = insert_into::<User>().values(Team { id: "team-1".to_string(), name: "Platform".to_string() });
}
