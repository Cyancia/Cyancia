pub trait WhenSome {
    fn when_some<T>(self, val: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self
    where
        Self: Sized;
}

impl<W> WhenSome for W {
    fn when_some<T>(self, val: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self
    where
        Self: Sized,
    {
        if let Some(val) = val {
            f(self, val)
        } else {
            self
        }
    }
}

pub trait When {
    fn when(self, condition: bool, f: impl FnOnce(Self) -> Self) -> Self
    where
        Self: Sized;
}

impl<W> When for W {
    fn when(self, condition: bool, f: impl FnOnce(Self) -> Self) -> Self
    where
        Self: Sized,
    {
        if condition { f(self) } else { self }
    }
}
