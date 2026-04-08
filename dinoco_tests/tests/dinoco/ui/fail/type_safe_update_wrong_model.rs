#[path = "../common.rs"]
mod common;

use dinoco::update;

use common::{Team, User};

fn main() {
    let _ = update::<User>().values(Team { id: "team-1".to_string(), name: "Platform".to_string() });
}
