#[path = "../common.rs"]
mod common;

use dinoco::find_many;

use common::{Team, User};

fn main() {
    let _ = find_many::<User>().select::<Team>();
}
