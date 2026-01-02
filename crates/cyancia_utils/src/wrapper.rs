#[macro_export]
macro_rules! wrapper {
    ($(#[$attr:meta])* $vis:vis $wrapper:ident $(<$($gen:tt),*>)? : $original: ty $(where $($bounds:tt)*)?) => {
        $(#[$attr])*
        $vis struct $wrapper $(<$($gen),*>)? ($original) $(where $($bounds)*)?;

        impl $(<$($gen),*>)? $wrapper $(<$($gen),*>)? $(where $($bounds)*)? {
            pub const fn new(value: $original) -> Self {
                Self(value)
            }
        }

        impl $(<$($gen),*>)? std::ops::Deref for $wrapper $(<$($gen),*>)? $(where $($bounds)*)? {
            type Target = $original;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl $(<$($gen),*>)? From<$original> for $wrapper $(<$($gen),*>)? $(where $($bounds)*)? {
            fn from(value: $original) -> Self {
                Self::new(value)
            }
        }
    };

    ($(#[$attr:meta])* $vis:vis mut $wrapper:ident $(<$($gen:tt),*>)? : $original: ty $(where $($bounds:tt)*)?) => {
        $crate::wrapper! {
            $(#[$attr])*
            $vis $wrapper $(<$($gen),*>)? : $original $(where $($bounds)*)?
        }

        impl $(<$($gen),*>)? std::ops::DerefMut for $wrapper $(<$($gen),*>)? $(where $($bounds)*)? {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
    };
}
