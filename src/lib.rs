#![allow(clippy::pedantic)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unused_async)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

/// app — mod for txtify.
pub mod app;
/// cli — mod for txtify.
pub mod cli;
/// config — mod for txtify.
pub mod config;
/// converters — mod for txtify.
pub mod converters;
/// core — mod for txtify.
pub mod core;
/// setup — mod for txtify.
pub mod setup;
/// shell — mod for txtify.
pub mod shell;

/// Embedded sidecar (build.rs also embeds sidecar/ via manifest)
pub const EMBEDDED_SIDECAR_PY: &str = include_str!("../sidecar/txtify_sidecar.py");
/// Embedded requirements
pub const EMBEDDED_SIDECAR_REQUIREMENTS: &str = include_str!("../sidecar/requirements.txt");

/// app — use for txtify.
pub use app::App;
/// core — use for txtify.
pub use core::error::TxtifyError;
