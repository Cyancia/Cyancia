use crate::{Application, Runtime, Services};

pub trait Plugin: 'static {
    fn build(&self, app: &mut Application);
    fn finish(&self, app: &mut Application) {}
}
