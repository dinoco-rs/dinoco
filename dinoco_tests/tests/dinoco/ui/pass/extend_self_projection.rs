#![allow(dead_code)]

use dinoco::{Extend, Model, ScalarField};

#[derive(Default)]
struct DeviceWhere {
    id: ScalarField<String>,
}

#[derive(Default)]
struct DeviceInclude {}

#[derive(Debug, Clone, Extend)]
#[extend(Device)]
struct Device {
    id: String,
}

impl Model for Device {
    type Include = DeviceInclude;
    type Where = DeviceWhere;

    fn table_name() -> &'static str {
        "Device"
    }
}

fn main() {
    let _ = dinoco::find_first::<Device>();
}
