#[macro_export]
macro_rules! ternary {
    ($cond:expr, $a:expr, $b:expr) => {
        if $cond { $a } else { $b }
    };
}
