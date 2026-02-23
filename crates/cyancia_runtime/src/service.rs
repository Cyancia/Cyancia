use downcast_rs::DowncastSync;

pub trait Service: DowncastSync {}

downcast_rs::impl_downcast!(sync Service);
