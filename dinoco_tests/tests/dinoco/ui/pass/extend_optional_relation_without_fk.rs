#![allow(dead_code)]
#![allow(non_snake_case)]

use dinoco::{
    DinocoAdapter, DinocoClient, Extend, IncludeLoaderFuture, Model, Projection, ReadMode, RelationField, Rowable,
    ScalarField,
};

#[derive(Debug, Clone, Rowable)]
struct Player {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Rowable)]
struct ResetPassword {
    id: String,
    playerId: String,
}

struct PlayerWhere {
    id: ScalarField<String>,
    name: ScalarField<String>,
}

struct ResetPasswordWhere {
    id: ScalarField<String>,
    playerId: ScalarField<String>,
}

#[derive(Default)]
struct PlayerInclude {}

#[derive(Default)]
struct ResetPasswordInclude {}

impl ResetPasswordInclude {
    fn player(&self) -> RelationField<Player> {
        RelationField::new("player")
    }
}

#[derive(Debug, Clone, Extend)]
#[extend(ResetPassword)]
struct ResetPasswordModel {
    id: String,
    player: Option<Player>,
}

impl Projection<Player> for Player {
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
}

impl Projection<ResetPassword> for ResetPassword {
    fn columns() -> &'static [&'static str] {
        &["id", "playerId"]
    }
}

impl Model for Player {
    type Include = PlayerInclude;
    type Where = PlayerWhere;

    fn table_name() -> &'static str {
        "players"
    }
}

impl Model for ResetPassword {
    type Include = ResetPasswordInclude;
    type Where = ResetPasswordWhere;

    fn table_name() -> &'static str {
        "reset_passwords"
    }
}

impl Default for PlayerWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), name: ScalarField::new("name") }
    }
}

impl Default for ResetPasswordWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), playerId: ScalarField::new("playerId") }
    }
}

impl ResetPassword {
    pub fn __dinoco_load_player_by_primary_key<'a, P, C, A>(
        _item_keys: Vec<Option<String>>,
        _include: &'a dinoco::IncludeNode,
        _client: &'a DinocoClient<A>,
        _read_mode: ReadMode,
        _relation_field: impl Fn(&mut P) -> &mut Option<C> + Copy + Send + 'a,
    ) -> IncludeLoaderFuture<'a, P>
    where
        A: DinocoAdapter,
        C: Projection<Player>,
    {
        Box::pin(async move { Ok(Box::new(|_: &mut [P]| {}) as dinoco::IncludeApplier<'a, P>) })
    }
}

fn main() {
    let _ = dinoco::find_first::<ResetPasswordModel>();
}
