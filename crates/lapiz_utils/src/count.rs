#[macro_export]
macro_rules! count {
    () => { 0 };
    ($($a:tt $b:tt $c:tt $d:tt)*) => {
        count!($($a)*) << 2
    };
    ($odd:tt $($a:tt $b:tt $c:tt $d:tt)*) => {
        (count!($($a)*) << 2) | 1
    };
    ($odd_1:tt $odd_2:tt $($a:tt $b:tt $c:tt $d:tt)*) => {
        (count!($($a)*) << 2) | 2
    };
    ($odd_1:tt $odd_2:tt $odd_3:tt $($a:tt $b:tt $c:tt $d:tt)*) => {
        (count!($($a)*) << 2) | 3
    };
}
