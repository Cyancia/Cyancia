#[macro_export]
macro_rules! wrapper {
    ($(#[$attr: meta])* $vis: vis $wrapper: ident : $original: ty) => {
        $(#[$attr])*
        $vis struct $wrapper($original);

        impl $wrapper {
            pub const fn new(value: $original) -> Self {
                Self(value)
            }
        }

        impl std::ops::Deref for $wrapper {
            type Target = $original;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl From<$original> for $wrapper {
            fn from(value: $original) -> Self {
                Self::new(value)
            }
        }
    };

    ($(#[$attr: meta])* $vis: vis mut $wrapper: ident : $original: ty) => {
        $crate::define_wrapper_ty! {
            $(#[$attr])*
            $vis $wrapper : $original
        }

        impl std::ops::DerefMut for $wrapper {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
    };
}
