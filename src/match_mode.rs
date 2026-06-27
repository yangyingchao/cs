use crate::stack_data::Frame;

/// Stack matching granularity
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum MatchMode {
    /// Match using all frame fields (address, depth, function, library)
    Precise,
    /// Match using function names only
    Fuzzy,
}

/// Key for dedup HashMap, supporting both match modes.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum StackKey {
    Full(Vec<Frame>),
    Signature(String),
}

impl MatchMode {
    pub fn build_key(&self, frames: &[Frame]) -> StackKey {
        match self {
            MatchMode::Precise => StackKey::Full(frames.to_vec()),
            MatchMode::Fuzzy => {
                let sig: Vec<&str> = frames.iter().map(|f| f.function.as_str()).collect();
                StackKey::Signature(sig.join(";"))
            }
        }
    }
}
