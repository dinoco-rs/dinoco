use etanol_macros::{impl_value_trait, impl_value_trait_option};

pub trait Value {
    fn toValue(&self, default: Option<String>) -> String;
}

impl_value_trait!(String);
impl_value_trait!(str);
impl_value_trait!(&str);
impl_value_trait!(bool);
impl_value_trait!(i64);

impl_value_trait_option!(String);
impl_value_trait_option!(&str);
impl_value_trait_option!(bool);
impl_value_trait_option!(i64);
