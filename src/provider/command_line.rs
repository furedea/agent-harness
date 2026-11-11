use clap::{ValueEnum, builder::PossibleValue};

use super::spec::ProviderId;

/// Written by hand: clap's derive emits an `allow` that `spec`'s `forbid` rejects.
impl ValueEnum for ProviderId {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Claude,
            Self::Codex,
            Self::Devin,
            Self::Hermes,
            Self::Pi,
            Self::Dsh,
        ]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.spec().key))
    }
}
