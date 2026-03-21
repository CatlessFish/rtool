use clap::{Args, Subcommand};

#[derive(Args, Debug, Clone)]
pub struct RtoolArgs {
    #[command(subcommand)]
    pub command: Commands,
    #[arg(long, help = "specify the timeout seconds in running rtool")]
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// perform analyses on the crate
    #[command(arg_required_else_help = true)]
    Analyze {
        #[command(subcommand)]
        kind: AnalysisKind,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum AnalysisKind {
    /// scan for potential deadlocks
    Deadlock {
        /// (optional) Save analyzed tags to JSON file
        #[arg(long)]
        save_tags: Option<String>,
        /// (optional) Load tags from JSON file
        #[arg(long)]
        load_tags: Option<String>,
    },
    /// print lock tag parsing results for development
    Dev,
    /// print MIR of the crate
    Mir {
        /// show MIR of every function
        #[arg(long)]
        all: bool,
        /// exact def_path or DefId substring match
        #[arg(long = "exact")]
        exact: Vec<String>,
        /// fuzzy match against the last path segment
        #[arg(long = "fuzzy")]
        fuzzy: Vec<String>,
        /// write MIR output to a file
        #[arg(long)]
        outpath: Option<String>,
    },
    /// query whether a call chain exists
    Callchain {
        /// source function name or def_path
        #[arg(long)]
        from: String,
        /// target function name or def_path
        #[arg(long)]
        to: String,
        /// print all matching call chains
        #[arg(long)]
        all_paths: bool,
        /// write call chain output to a file
        #[arg(long)]
        outpath: Option<String>,
    },
}
