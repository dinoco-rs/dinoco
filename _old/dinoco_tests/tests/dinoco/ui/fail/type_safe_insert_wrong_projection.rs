#[path = "../common.rs"]
mod common;

use dinoco::insert_into;

use common::{Team, User};

fn main() {
    let _ = insert_into::<User>().values(User { id: 1, name: "Matheus".to_string() }).returning::<Team>();
}
