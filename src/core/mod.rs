/// detector — mod for txtify.
pub mod detector;
/// error — mod for txtify.
pub mod error;
/// registry — mod for txtify.
pub mod registry;
/// traits — mod for txtify.
pub mod traits;
/// types — mod for txtify.
pub mod types;

/// detector — use for txtify.
pub use detector::DefaultDetector;
/// error — use for txtify.
pub use error::TxtifyError;
/// registry — use for txtify.
pub use registry::ConverterRegistry;
/// traits — use for txtify.
pub use traits::{Converter, FormatDetector};
/// types — use for txtify.
pub use types::{
    ConversionMetadata, ConversionMode, ConversionRequest, ConversionResult, GlmEngine,
    ImageFormat, InputFormat, OutputFormat,
};
