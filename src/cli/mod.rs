use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// Txtify - Hybrid document-to-text converter.
#[derive(Debug, Parser)]
#[command(name = "txtify", version, about = "Hybrid document-to-text converter", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Input file to convert (legacy top-level, use `convert` subcommand)
    #[arg(value_name = "INPUT", hide = true)]
    pub input: Option<PathBuf>,

    /// Output file (legacy top-level)
    #[arg(short, long, hide = true)]
    pub output: Option<PathBuf>,

    /// Conversion mode (legacy top-level)
    #[arg(short, long, value_enum, default_value_t = Mode::Fast, hide = true)]
    pub mode: Mode,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
/// Mode — enum for txtify.
pub enum Mode {
    Fast,
    #[value(name = "high-quality")]
    HighQuality,
}

impl Mode {
    /// to_conversion_mode — fn for txtify.
    pub fn to_conversion_mode(self) -> crate::core::types::ConversionMode {
        match self {
            Self::Fast => crate::core::types::ConversionMode::Fast,
            Self::HighQuality => crate::core::types::ConversionMode::HighQuality,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
/// OutputFormatArg — enum for txtify.
pub enum OutputFormatArg {
    #[default]
    #[value(name = "md")]
    Md,
    #[value(name = "txt")]
    Txt,
    #[value(name = "json")]
    Json,
}

impl OutputFormatArg {
    /// to_core — fn for txtify.
    pub fn to_core(self) -> crate::core::types::OutputFormat {
        match self {
            Self::Md => crate::core::types::OutputFormat::Md,
            Self::Txt => crate::core::types::OutputFormat::Txt,
            Self::Json => crate::core::types::OutputFormat::Json,
        }
    }

    /// extension — fn for txtify.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Md => "md",
            Self::Txt => "txt",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
/// Backend — enum for txtify.
pub enum Backend {
    #[default]
    #[value(name = "auto")]
    Auto,
    #[value(name = "vllm")]
    Vllm,
    #[value(name = "transformers")]
    Transformers,
    #[value(name = "sglang")]
    Sglang,
    #[value(name = "ollama")]
    Ollama,
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Auto => "auto",
            Self::Vllm => "vllm",
            Self::Transformers => "transformers",
            Self::Sglang => "sglang",
            Self::Ollama => "ollama",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Subcommand)]
/// Commands — enum for txtify.
pub enum Commands {
    /// Convert one or more documents to text
    Convert {
        /// Input file(s) to convert
        #[arg(value_name = "INPUT", required = true)]
        input: Vec<PathBuf>,

        /// Output file or directory. Use '-' for stdout. If multiple inputs and -o is a directory, files are written inside it.
        #[arg(short, long, value_name = "OUT")]
        output: Option<PathBuf>,

        /// Output format
        #[arg(long = "to", value_enum, default_value_t = OutputFormatArg::Md)]
        to: OutputFormatArg,

        /// Conversion mode
        #[arg(long, value_enum, default_value_t = Mode::Fast)]
        mode: Mode,

        /// GLM backend
        #[arg(long, value_enum, default_value_t = Backend::Auto)]
        backend: Backend,

        /// Overwrite existing output files
        #[arg(long, conflicts_with = "skip_existing")]
        overwrite: bool,

        /// Skip existing output files
        #[arg(long, conflicts_with = "overwrite")]
        skip_existing: bool,
    },

    /// Batch convert all files in a folder
    Batch {
        /// Folder to batch convert
        #[arg(value_name = "FOLDER")]
        folder: PathBuf,

        /// Recurse into subdirectories
        #[arg(long)]
        recursive: bool,

        /// Output format
        #[arg(long = "to", value_enum, default_value_t = OutputFormatArg::Md)]
        to: OutputFormatArg,

        /// Conversion mode
        #[arg(long, value_enum, default_value_t = Mode::Fast)]
        mode: Mode,

        /// GLM backend
        #[arg(long, value_enum, default_value_t = Backend::Auto)]
        backend: Backend,

        /// Output directory (defaults to alongside input files)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Overwrite existing files
        #[arg(long, conflicts_with = "skip_existing")]
        overwrite: bool,

        /// Skip existing files
        #[arg(long, conflicts_with = "overwrite")]
        skip_existing: bool,
    },

    /// Check system dependencies and sidecar health
    Doctor,

    /// Download all resources (Python deps, AI model ~1GB, config) — used by curl installer
    Setup {
        /// HuggingFace model ID to prefetch (default: zai-org/GLM-OCR)
        #[arg(long)]
        model: Option<String>,

        /// GLM backend for config (auto|transformers|vllm|sglang|ollama)
        #[arg(long)]
        backend: Option<String>,

        /// Python executable (default: auto-detect)
        #[arg(long)]
        python: Option<String>,

        /// Skip AI model download (~1GB)
        #[arg(long)]
        no_model: bool,

        /// Skip `pip install` of Python dependencies
        #[arg(long)]
        no_python_deps: bool,

        /// Also install shell integration (context menu)
        #[arg(long)]
        with_shell: bool,

        /// Non-interactive (no prompts)
        #[arg(long)]
        yes: bool,

        /// Only check, change nothing
        #[arg(long)]
        check: bool,
    },

    /// Show config and paths
    Config,

    /// Show version information (legacy)
    Version,

    /// Shell integration (context menu / file manager actions)
    Shell {
        #[command(subcommand)]
        command: ShellCommands,
    },
}

#[derive(Debug, Subcommand)]
/// ShellCommands — enum for txtify.
pub enum ShellCommands {
    /// Install shell integration (context menu)
    Install,
    /// Uninstall shell integration
    Uninstall,
    /// Show shell integration status
    Status,
}
