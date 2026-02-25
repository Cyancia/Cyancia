use crate::{Application, Runtime};

pub trait Plugin: 'static {
    fn build(&self, app: &mut Application);
    fn finish(&self, app: &mut Application) {}
}
