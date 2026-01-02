use downcast_rs::Downcast;

pub trait Asset: Send + Sync + 'static + Downcast {}

downcast_rs::impl_downcast!(Asset);

pub trait ErasedAsset: Downcast {}

impl<T: Asset> ErasedAsset for T {}

downcast_rs::impl_downcast!(ErasedAsset);
