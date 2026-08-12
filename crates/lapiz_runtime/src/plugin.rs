use crate::Application;

pub trait Plugin: 'static {
    fn build(&self, app: &mut Application);
    fn finish(&self, _app: &mut Application) {}
}
