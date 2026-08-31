/// fast — mod for txtify.
pub mod fast;
/// pandoc — mod for txtify.
pub mod pandoc;
/// sidecar — mod for txtify.
pub mod sidecar;

/// fast — use for txtify.
pub use fast::FastConverter;
/// fast — use for txtify.
pub use fast::{
    DocxConverter, HtmlConverter, PdfConverter, PptxConverter, TextConverter, XlsxConverter,
};
/// pandoc — use for txtify.
pub use pandoc::PandocConverter;
/// sidecar — use for txtify.
pub use sidecar::SidecarConverter;
/// sidecar — use for txtify.
pub use sidecar::{GlmOcrConverter, SidecarClient};
