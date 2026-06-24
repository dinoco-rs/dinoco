#[path = "../common.rs"]
mod common;

use dinoco::{count, delete, delete_many, find_first, find_many, insert_into, insert_many, update, update_many};

use common::{User, UserSummary};

fn main() {
    let user_insert = User { id: 1, name: "Matheus".to_string() };
    let user_update = User { id: 1, name: "Updated".to_string() };
    let user_batch_a = User { id: 2, name: "Ana".to_string() };
    let user_batch_b = User { id: 3, name: "Caio".to_string() };

    let _ = insert_into::<User>().values(User { id: 1, name: "Matheus".to_string() }).returning_as::<UserSummary>();
    let _ = insert_into::<User>().values(&user_insert).returning_as::<UserSummary>();
    let _ = insert_many::<User>()
        .values(vec![User { id: 2, name: "Ana".to_string() }, User { id: 3, name: "Caio".to_string() }])
        .returning();
    let _ = insert_many::<User>().values(vec![&user_batch_a, &user_batch_b]).returning();

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

    let _ = update::<User>()
        .cond(|x| x.id.eq(1_i64))
        .update(|x| x.name.set("Updated"))
        .returning_as::<UserSummary>();
    let _ = update::<User>().cond(|x| x.id.eq(1_i64)).update(|x| x.name.set(&user_update.name)).returning_as::<UserSummary>();
    let _ = update_many::<User>()
        .update(|x| x.name.set("Batch"))
        .returning();
    let _ = update_many::<User>().update(|x| x.name.set(&user_batch_a.name)).returning();

    let _ = delete::<User>().cond(|x| x.id.eq(1_i64));
    let _ = delete_many::<User>().cond(|x| x.name.starts_with("A"));
}
