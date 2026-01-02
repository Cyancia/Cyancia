use crate::shell::ActionShell;

pub trait ActionTask: Send + Sync + 'static {
    fn apply(self: Box<Self>, shell: &mut ActionShell);
}

impl<T: ActionTask> ActionTask for Option<T> {
    fn apply(self: Box<Self>, shell: &mut ActionShell) {
        if let Some(task) = *self {
            <T as ActionTask>::apply(Box::new(task), shell);
        }
    }
}

impl ActionTask for () {
    fn apply(self: Box<Self>, _shell: &mut ActionShell) {}
}
