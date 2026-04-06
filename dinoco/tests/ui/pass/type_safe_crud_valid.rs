#[path = "../common.rs"]
mod common;

use dinoco::{count, delete, delete_many, find_first, find_many, insert_into, insert_many, update, update_many};

use common::{User, UserSummary};

fn main() {
    let _ = insert_into::<User>().values(User { id: 1, name: "Matheus".to_string() });
    let _ = insert_many::<User>().values(vec![
        User { id: 2, name: "Ana".to_string() },
        User { id: 3, name: "Caio".to_string() },
    ]);

    let _ = find_first::<User>().select::<UserSummary>().cond(|x| x.id.eq(1_i64));
    let _ = find_many::<User>()
        .select::<UserSummary>()
        .cond(|x| x.name.includes("in"))
        .cond(|x| x.id.in_values(vec![1_i64, 2_i64]))
        .cond(|x| x.id.not_in_values(vec![3_i64]))
        .order_by(|x| x.id.asc())
        .take(10)
        .skip(1);
    let _ = count::<User>().cond(|x| x.name.includes("in"));

    let _ = update::<User>().cond(|x| x.id.eq(1_i64)).values(User { id: 1, name: "Updated".to_string() });
    let _ = update_many::<User>().values(vec![
        User { id: 2, name: "Ana Batch".to_string() },
        User { id: 3, name: "Caio Batch".to_string() },
    ]);

    let _ = delete::<User>().cond(|x| x.id.eq(1_i64));
    let _ = delete_many::<User>().cond(|x| x.name.starts_with("A"));
}
