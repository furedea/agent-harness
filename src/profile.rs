use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Profile {
    Minimal,
    Furedea,
}

impl Profile {
    pub(crate) fn directory_name(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Furedea => "furedea",
        }
    }
}
