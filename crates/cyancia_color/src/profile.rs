use cyancia_utils::wrapper;
use parse_display::Display;

wrapper! {
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Display)]
    pub ColorProfileId : &'static str
}
