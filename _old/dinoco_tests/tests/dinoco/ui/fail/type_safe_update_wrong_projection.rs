#[path = "../common.rs"]
mod common;

use dinoco::update;

use common::{Team, User};

fn main() {
    let _ =
        update::<User>().cond(|x| x.id.eq(1_i64)).values(User { id: 1, name: "Updated".to_string() }).returning::<Team>();
}
