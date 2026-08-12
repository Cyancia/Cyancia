#[macro_export]
macro_rules! include_shader {
    ($($component: literal)+) => {
        include_str!(concat!(env!("OUT_DIR"), "/", $($component)+))
    };
}
