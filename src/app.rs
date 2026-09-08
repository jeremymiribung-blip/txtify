use std::path::{Path, PathBuf};
use std::sync::Arc;

use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::cli::{Backend, Cli, Commands, OutputFormatArg};
use crate::config::Config;
use crate::converters::sidecar::{detect_python, find_sidecar_script, SidecarClient};
use crate::converters::{FastConverter, PandocConverter, SidecarConverter};
use crate::core::detector::DefaultDetector;
use crate::core::error::TxtifyError;
use crate::core::registry::ConverterRegistry;
use crate::core::traits::FormatDetector;
use crate::core::types::{ConversionMode, ConversionRequest, OutputFormat};

/// Application entry point logic.
pub struct App {
    config: Config,
}

impl App {
    /// new — fn for txtify.
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// from_config — fn for txtify.
    pub fn from_config() -> Result<Self, TxtifyError> {
        let config = Config::load()?;
        Ok(Self::new(config))
    }

    /// run — documented.
    pub async fn run(&self, cli: Cli) -> Result<(), TxtifyError> {
        tracing::debug!(?cli, ?self.config, "running app");

        match cli.command {
            Some(Commands::Convert {
                input,
                output,
                to,
                mode,
                backend,
                overwrite,
                skip_existing,
            }) => {
                self.run_convert(input, output, to, mode, backend, overwrite, skip_existing)
                    .await
            }
            Some(Commands::Batch {
                folder,
                recursive,
                to,
                mode,
                backend,
                output,
                overwrite,
                skip_existing,
            }) => {
                self.run_batch(
                    folder,
                    recursive,
                    to,
                    mode,
                    backend,
                    output,
                    overwrite,
                    skip_existing,
                )
                .await
            }
            Some(Commands::Doctor) => self.run_doctor().await,
            Some(Commands::Setup {
                model,
                backend,
                python,
                no_model,
                no_python_deps,
                with_shell,
                yes,
                check,
            }) => {
                let opts = crate::setup::SetupOptions {
                    model,
                    backend,
                    python,
                    no_model,
                    no_python_deps,
                    with_shell,
                    yes,
                    check_only: check,
                };
                crate::setup::run_setup(&self.config, &opts).await
            }
            Some(Commands::Config) => self.run_config().await,
            Some(Commands::Version) => {
                println!("txtify {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
            Some(Commands::Shell { command }) => self.run_shell(command).await,
            None => {
                if let Some(input) = cli.input {
                    // Legacy top-level single file convert to stdout or file
                    tracing::info!(?input, output = ?cli.output, mode = ?cli.mode, "convert via top-level args");
                    let to = OutputFormatArg::Md;
                    let backend = Backend::Auto;
                    self.run_convert(vec![input], cli.output, to, cli.mode, backend, false, false)
                        .await
                } else {
                    // No subcommand and no input: print help hint
                    // Clap would normally handle --help, but if called without args we show hint
                    println!(
                        "txtify {} - Hybrid document-to-text converter",
                        env!("CARGO_PKG_VERSION")
                    );
                    println!("Run `txtify --help` for usage.");
                    Ok(())
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_convert(
        &self,
        inputs: Vec<PathBuf>,
        output: Option<PathBuf>,
        to: OutputFormatArg,
        mode: crate::cli::Mode,
        backend: Backend,
        overwrite: bool,
        skip_existing: bool,
    ) -> Result<(), TxtifyError> {
        if inputs.is_empty() {
            return Err(TxtifyError::ConversionFailed(
                "no input files provided".to_string(),
            ));
        }

        let detector = DefaultDetector;
        let registry = Self::build_registry(&self.config, backend);

        let conversion_mode = mode.to_conversion_mode();
        let output_format = to.to_core();

        // Determine stdout cases
        let is_stdout_dash = output.as_ref().is_some_and(|p| p.as_os_str() == "-");
        let is_single_no_output = output.is_none() && inputs.len() == 1 && !is_stdout_dash;

        // If stdout mode (dash or single no output), convert first file and print to stdout
        if is_stdout_dash || is_single_no_output {
            let input_path = &inputs[0];
            // Warn if multiple inputs but stdout requested: only first? Better to handle sequentially to stdout with separators
            if inputs.len() > 1 && is_stdout_dash {
                // Concatenate to stdout with progress? We will convert sequentially and write each to stdout with separator
                for inp in &inputs {
                    let result = self
                        .convert_one(
                            &detector,
                            &registry,
                            inp,
                            None,
                            output_format,
                            conversion_mode,
                        )
                        .await?;
                    let rendered =
                        Self::render_output(&result.markdown, &result.metadata, output_format)?;
                    // Write to stdout
                    use std::io::Write;
                    let mut stdout = std::io::stdout();
                    let _ = stdout.write_all(rendered.as_bytes());
                    let _ = stdout.write_all(b"\n");
                    tracing::info!(input = %inp.display(), "converted to stdout");
                }
                return Ok(());
            }
            if inputs.len() > 1 && is_single_no_output {
                return Err(TxtifyError::ConversionFailed(
                    "multiple inputs require -o <OUT> directory or -o - for stdout".to_string(),
                ));
            }
            let result = self
                .convert_one(
                    &detector,
                    &registry,
                    input_path,
                    None,
                    output_format,
                    conversion_mode,
                )
                .await?;
            let rendered = Self::render_output(&result.markdown, &result.metadata, output_format)?;
            // Write to stdout without extra file I/O
            use std::io::Write;
            let mut stdout = std::io::stdout();
            stdout
                .write_all(rendered.as_bytes())
                .map_err(TxtifyError::Io)?;
            if !rendered.ends_with('\n') {
                let _ = stdout.write_all(b"\n");
            }
            return Ok(());
        }

        // Multiple inputs handling with output directory or per-file
        let concurrency = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut join_set = JoinSet::new();

        // Determine if output is directory
        let output_is_dir = output.as_ref().is_some_and(|p| {
            p.is_dir() || (p.exists() && p.is_dir()) || Self::is_dir_like(p, &inputs)
        });

        // Deduplicate targets for same-stem inputs (e.g. sample.docx + sample.xlsx -> sample.md collision)
        let mut claimed: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        let mut planned: Vec<(PathBuf, Option<PathBuf>)> = Vec::with_capacity(inputs.len());
        for inp in &inputs {
            let raw_target = Self::resolve_target_path(inp, output.as_deref(), to, output_is_dir);
            let unique_target = if let Some(t) = raw_target {
                if t.as_os_str() == "-" {
                    Some(t)
                } else {
                    let mut candidate = t.clone();
                    let mut counter = 1;
                    // Also check filesystem exists without --overwrite/--skip to avoid silent overwrite
                    while claimed.contains(&candidate) {
                        let stem = inp
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "output".to_string());
                        let dir = candidate.parent().unwrap_or_else(|| Path::new(""));
                        candidate = dir.join(format!("{stem}_{}.{}", counter, to.extension()));
                        counter += 1;
                    }
                    claimed.insert(candidate.clone());
                    Some(candidate)
                }
            } else {
                // alongside: input.with_extension - also dedup
                let raw = inp.with_extension(to.extension());
                let mut cand = raw.clone();
                let mut c = 1;
                while claimed.contains(&cand) {
                    let stem = inp
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "output".to_string());
                    let parent = inp.parent().unwrap_or_else(|| Path::new(""));
                    cand = parent.join(format!("{stem}_{}.{}", c, to.extension()));
                    c += 1;
                }
                claimed.insert(cand.clone());
                Some(cand)
            };
            // For the case output is None we already produced a target, but run_convert's logic expects None for alongside
            // Keep None sentinel for alongside to preserve original behavior, but use deduplicated path via claimed set
            // So if original output was None, keep None yet reserve claimed path
            let effective = if output.is_none() && !output_is_dir {
                // Convert deduped path back to None? Actually we need to pass the deduped alongside path as target
                // Use the deduped candidate directly
                unique_target
            } else {
                unique_target
            };
            planned.push((inp.clone(), effective));
        }

        for (input_path, dedup_target) in planned {
            let sem = semaphore.clone();
            let out_format = output_format;
            let out_to_arg = to;
            // Clone needed data for async block: we need to create new detector/registry per task? They are stateless, we can share via Arc or recreate
            // For simplicity, build registry per task or share via Arc (registry is not Sync due to Box<dyn Converter> not clone). Recreate per task is cheap.
            let cfg = self.config.clone();
            let backend_clone = backend;
            let ow = overwrite;
            let skip = skip_existing;
            let conv_mode = conversion_mode;
            let target_for_task = dedup_target.clone();

            join_set.spawn(async move {
                let _permit = sem.acquire_owned().await.map_err(|e| {
                    TxtifyError::ConversionFailed(format!("semaphore closed: {e}"))
                })?;
                let detector = DefaultDetector;
                let registry = Self::build_registry(&cfg, backend_clone);

                // Use pre-deduplicated target
                let target = target_for_task;

                // Check overwrite/skip
                if let Some(ref tgt) = target {
                    if tgt.as_os_str() != "-" && tgt.exists() {
                        if skip {
                            tracing::info!(input = %input_path.display(), target = %tgt.display(), "skip existing");
                            return Ok::<Option<PathBuf>, TxtifyError>(None);
                        }
                        if !ow {
                            return Err(TxtifyError::ConversionFailed(format!(
                                "output exists: {} (use --overwrite or --skip-existing)",
                                tgt.display()
                            )));
                        }
                    }
                }

                let result = Self::convert_one_static(
                    &detector,
                    &registry,
                    &input_path,
                    target.as_deref(),
                    out_format,
                    conv_mode,
                )
                .await?;

                let rendered = Self::render_output(&result.markdown, &result.metadata, out_format)?;

                if let Some(tgt) = target {
                    if tgt.as_os_str() == "-" {
                        use std::io::Write;
                        let mut stdout = std::io::stdout();
                        stdout
                            .write_all(rendered.as_bytes())
                            .map_err(TxtifyError::Io)?;
                    } else {
                        if let Some(parent) = tgt.parent() {
                            if !parent.as_os_str().is_empty() && !parent.exists() {
                                std::fs::create_dir_all(parent).map_err(TxtifyError::Io)?;
                            }
                        }
                        std::fs::write(&tgt, rendered.as_bytes()).map_err(TxtifyError::Io)?;
                        tracing::info!(input = %input_path.display(), output = %tgt.display(), "converted");
                    }
                    Ok(Some(tgt))
                } else {
                    // No target, write alongside
                    let tgt = input_path.with_extension(out_to_arg.extension());
                    if tgt.exists() && skip {
                        return Ok(None);
                    }
                    if tgt.exists() && !ow {
                        return Err(TxtifyError::ConversionFailed(format!(
                            "output exists: {} (use --overwrite)",
                            tgt.display()
                        )));
                    }
                    if let Some(parent) = tgt.parent() {
                        if !parent.as_os_str().is_empty() && !parent.exists() {
                            std::fs::create_dir_all(parent).map_err(TxtifyError::Io)?;
                        }
                    }
                    std::fs::write(&tgt, rendered.as_bytes()).map_err(TxtifyError::Io)?;
                    Ok(Some(tgt))
                }
            });
        }

        let mut errors = Vec::new();
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => errors.push(e.to_string()),
                Err(e) => errors.push(format!("task join error: {e}")),
            }
        }

        if !errors.is_empty() {
            return Err(TxtifyError::ConversionFailed(errors.join("; ")));
        }

        Ok(())
    }

    fn is_dir_like(p: &Path, inputs: &[PathBuf]) -> bool {
        // Heuristic: if p has no extension and inputs >1, treat as dir
        // Or if path ends with / or is existing dir
        if inputs.len() > 1 {
            // If output path has no extension or ends with separator, likely dir
            if p.extension().is_none() {
                return true;
            }
        }
        false
    }

    fn resolve_target_path(
        input: &Path,
        output: Option<&Path>,
        to: OutputFormatArg,
        output_is_dir: bool,
    ) -> Option<PathBuf> {
        match output {
            None => None,
            Some(out) => {
                if out.as_os_str() == "-" {
                    return Some(PathBuf::from("-"));
                }
                if output_is_dir {
                    // Ensure we keep stem only, then add extension
                    let stem = input
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "output".to_string());
                    let dir = out;
                    return Some(dir.join(format!("{stem}.{}", to.extension())));
                }
                // Single output file case: if output is file path, use it directly for single input?
                // For single input, output may be file; for multi, already handled as dir above
                Some(out.to_path_buf())
            }
        }
    }

    async fn convert_one(
        &self,
        detector: &DefaultDetector,
        registry: &ConverterRegistry,
        input_path: &Path,
        output_path: Option<PathBuf>,
        output_format: OutputFormat,
        mode: ConversionMode,
    ) -> Result<crate::core::types::ConversionResult, TxtifyError> {
        Self::convert_one_static(
            detector,
            registry,
            input_path,
            output_path.as_deref(),
            output_format,
            mode,
        )
        .await
    }

    async fn convert_one_static(
        detector: &DefaultDetector,
        registry: &ConverterRegistry,
        input_path: &Path,
        output_path: Option<&Path>,
        output_format: OutputFormat,
        mode: ConversionMode,
    ) -> Result<crate::core::types::ConversionResult, TxtifyError> {
        let input_format = detector.detect(input_path)?;
        let req = ConversionRequest {
            input_path: input_path.to_path_buf(),
            output_path: output_path.map(|p| p.to_path_buf()),
            input_format,
            output_format,
            mode,
        };
        let converter = registry.select(&req)?;
        converter.convert(&req).await
    }

    fn render_output(
        markdown: &str,
        metadata: &crate::core::types::ConversionMetadata,
        output_format: OutputFormat,
    ) -> Result<String, TxtifyError> {
        match output_format {
            OutputFormat::Json => {
                let val = serde_json::json!({
                    "markdown": markdown,
                    "metadata": metadata,
                });
                serde_json::to_string_pretty(&val)
                    .map_err(|e| TxtifyError::ConfigError(e.to_string()))
            }
            OutputFormat::Md | OutputFormat::Txt => Ok(markdown.to_string()),
        }
    }

    fn build_registry(config: &Config, backend_override: Backend) -> ConverterRegistry {
        let mut registry = ConverterRegistry::new();
        registry.register(Box::new(FastConverter));
        registry.register(Box::new(PandocConverter));
        // Determine sidecar config with backend override
        let mut sidecar_cfg = config.sidecar.clone();
        if backend_override != Backend::Auto {
            sidecar_cfg.glm_backend = backend_override.to_string();
        }
        // Register GLM sidecar (CPU inference is slow: honor timeout_secs, default 600s)
        let timeout = std::time::Duration::from_secs(config.timeout_secs.unwrap_or(600));
        let sidecar_converter =
            crate::converters::sidecar::GlmOcrConverter::from_config(&sidecar_cfg)
                .with_timeout(timeout);
        registry.register(Box::new(sidecar_converter));
        // Also register legacy sidecar name for completeness
        registry.register(Box::new(SidecarConverter::default()));
        registry
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_batch(
        &self,
        folder: PathBuf,
        recursive: bool,
        to: OutputFormatArg,
        mode: crate::cli::Mode,
        backend: Backend,
        output: Option<PathBuf>,
        overwrite: bool,
        skip_existing: bool,
    ) -> Result<(), TxtifyError> {
        if !folder.exists() {
            return Err(TxtifyError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("folder not found: {}", folder.display()),
            )));
        }
        if !folder.is_dir() {
            return Err(TxtifyError::ConversionFailed(format!(
                "not a directory: {}",
                folder.display()
            )));
        }

        let files = Self::collect_files(&folder, recursive)?;
        if files.is_empty() {
            println!("No files found in {}", folder.display());
            return Ok(());
        }

        let filtered: Vec<PathBuf> = files
            .into_iter()
            .filter(|p| {
                // Filter by supported extensions or detectable formats
                crate::core::types::InputFormat::from_extension(p).is_some()
            })
            .collect();

        if filtered.is_empty() {
            println!("No convertible files found in {}", folder.display());
            return Ok(());
        }

        println!(
            "Batch converting {} files from {} (mode: {:?}, to: {:?})",
            filtered.len(),
            folder.display(),
            mode,
            to
        );

        let pb = ProgressBar::new(filtered.len() as u64);
        let style = ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar());
        pb.set_style(style);

        let concurrency = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut join_set = JoinSet::new();

        let conversion_mode = mode.to_conversion_mode();
        let output_format = to.to_core();

        // Determine output base: if provided use it, else write alongside input with new extension
        let output_base = output.clone();

        // Deduplicate batch targets (same-stem collision, e.g. sample.docx + sample.xlsx -> sample.md)
        let mut claimed_batch: std::collections::HashSet<PathBuf> =
            std::collections::HashSet::new();
        let mut filtered_dedup: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(filtered.len());
        for p in filtered {
            let raw = if let Some(ref base) = output_base {
                let rel = p.strip_prefix(&folder).unwrap_or(p.as_path());
                let mut tgt = base.join(rel);
                tgt.set_extension(to.extension());
                tgt
            } else {
                let mut tgt = p.clone();
                tgt.set_extension(to.extension());
                tgt
            };
            let mut cand = raw.clone();
            let mut counter = 1;
            while claimed_batch.contains(&cand) {
                let parent = cand.parent().unwrap_or_else(|| Path::new(""));
                let stem = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "output".to_string());
                cand = parent.join(format!("{stem}_{}.{}", counter, to.extension()));
                counter += 1;
            }
            claimed_batch.insert(cand.clone());
            filtered_dedup.push((p, cand));
        }

        for (input_path, dedup_target) in filtered_dedup {
            let pb_clone = pb.clone();
            let sem = semaphore.clone();
            let cfg = self.config.clone();
            let backend_c = backend;
            let ow = overwrite;
            let skip = skip_existing;
            let out_format = output_format;
            let conv_mode = conversion_mode;

            join_set.spawn(async move {
                let _permit = sem
                    .acquire_owned()
                    .await
                    .map_err(|e| TxtifyError::ConversionFailed(format!("semaphore error: {e}")))?;

                let detector = DefaultDetector;
                let registry = Self::build_registry(&cfg, backend_c);

                // Use pre-deduplicated target (always Some)
                let target: PathBuf = dedup_target;

                if target.exists() {
                    if skip {
                        pb_clone.set_message(format!("skip {}", input_path.display()));
                        pb_clone.inc(1);
                        return Ok::<Option<PathBuf>, TxtifyError>(None);
                    }
                    if !ow {
                        pb_clone.inc(1);
                        return Err(TxtifyError::ConversionFailed(format!(
                            "output exists: {} (use --overwrite)",
                            target.display()
                        )));
                    }
                }

                // Detect and convert
                let res = match detector.detect(&input_path) {
                    Ok(fmt) => {
                        let req = ConversionRequest {
                            input_path: input_path.clone(),
                            output_path: Some(target.clone()),
                            input_format: fmt,
                            output_format: out_format,
                            mode: conv_mode,
                        };
                        match registry.select(&req) {
                            Ok(conv) => conv.convert(&req).await,
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => Err(e),
                };

                match res {
                    Ok(result) => {
                        let rendered =
                            Self::render_output(&result.markdown, &result.metadata, out_format)?;
                        if let Some(parent) = target.parent() {
                            if !parent.as_os_str().is_empty() && !parent.exists() {
                                std::fs::create_dir_all(parent).map_err(TxtifyError::Io)?;
                            }
                        }
                        std::fs::write(&target, rendered.as_bytes()).map_err(TxtifyError::Io)?;
                        pb_clone.set_message(format!("done {}", input_path.display()));
                        pb_clone.inc(1);
                        Ok(Some(target))
                    }
                    Err(e) => {
                        pb_clone.set_message(format!("err {}", input_path.display()));
                        pb_clone.inc(1);
                        // For batch, we log error but continue; return Err to be collected
                        Err(e)
                    }
                }
            });
        }

        let mut errors = Vec::new();
        let mut succeeded = 0usize;
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(Ok(Some(_))) => succeeded += 1,
                Ok(Ok(None)) => succeeded += 1, // skipped counts as success
                Ok(Err(e)) => errors.push(e.to_string()),
                Err(e) => errors.push(format!("join error: {e}")),
            }
        }
        pb.finish_with_message(format!("done {succeeded} files"));

        if !errors.is_empty() {
            eprintln!("Batch completed with {} errors:", errors.len());
            for err in &errors {
                eprintln!("  - {err}");
            }
            // Do not fail whole batch if some succeeded? Return error if all failed?
            // We return error only if none succeeded
            if succeeded == 0 {
                return Err(TxtifyError::ConversionFailed(errors.join("; ")));
            }
        }

        println!("Batch finished: {succeeded} files converted.");
        // Print output location hint
        if let Some(base) = output_base {
            println!("Output directory: {}", base.display());
        }

        Ok(())
    }

    fn collect_files(folder: &Path, recursive: bool) -> Result<Vec<PathBuf>, TxtifyError> {
        let mut files = Vec::new();
        Self::collect_files_inner(folder, recursive, &mut files)?;
        Ok(files)
    }

    fn collect_files_inner(
        dir: &Path,
        recursive: bool,
        out: &mut Vec<PathBuf>,
    ) -> Result<(), TxtifyError> {
        let entries = std::fs::read_dir(dir).map_err(TxtifyError::Io)?;
        for entry in entries {
            let entry = entry.map_err(TxtifyError::Io)?;
            let path = entry.path();
            let meta = entry.metadata().map_err(TxtifyError::Io)?;
            if meta.is_file() {
                out.push(path);
            } else if meta.is_dir() && recursive {
                Self::collect_files_inner(&path, true, out)?;
            }
        }
        Ok(())
    }

    async fn run_doctor(&self) -> Result<(), TxtifyError> {
        println!("txtify doctor");
        println!("=============");

        // Config paths
        println!("\n[config]");
        for p in Config::candidate_paths() {
            let status = if p.exists() { "found" } else { "not found" };
            println!("  {} : {}", p.display(), status);
        }
        if let Some(resolved) = Config::resolved_path() {
            println!("  resolved: {}", resolved.display());
        } else {
            println!("  resolved: (none, using defaults)");
        }
        println!(
            "  config: {}",
            self.config.to_json().unwrap_or_else(|_| "{}".to_string())
        );

        // Pandoc
        println!("\n[pandoc]");
        let pandoc_available = PandocConverter::is_available().await;
        if pandoc_available {
            println!("  pandoc: available");
            // Try version
            if let Ok(output) = tokio::process::Command::new("pandoc")
                .arg("--version")
                .output()
                .await
            {
                let ver = String::from_utf8_lossy(&output.stdout);
                let first = ver.lines().next().unwrap_or("");
                println!("  version: {first}");
            }
        } else {
            println!("  pandoc: not found (install from https://pandoc.org/installing.html)");
        }

        // Python
        println!("\n[python]");
        let python_path = self
            .config
            .sidecar
            .python_path
            .clone()
            .unwrap_or_else(detect_python);
        println!("  python_path: {python_path}");
        let python_exists = if python_path.contains('/') {
            Path::new(&python_path).exists()
        } else {
            true
        };
        if python_exists {
            match tokio::process::Command::new(&python_path)
                .arg("--version")
                .output()
                .await
            {
                Ok(out) if out.status.success() => {
                    let ver = String::from_utf8_lossy(&out.stdout);
                    let ver2 = String::from_utf8_lossy(&out.stderr);
                    let combined = format!("{ver}{ver2}");
                    println!("  python: available ({})", combined.trim());
                }
                Ok(out) => {
                    println!(
                        "  python: found but error: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                }
                Err(e) => {
                    println!("  python: not executable: {e}");
                }
            }
        } else {
            println!("  python: not found at {python_path} (hint: set sidecar.python_path in config or install python)");
        }

        // GLM-OCR model
        println!("\n[glm-ocr]");
        println!("  model: {}", self.config.sidecar.glm_model);
        println!("  backend: {}", self.config.sidecar.glm_backend);
        // Check python import
        let check_import = tokio::process::Command::new(&python_path)
            .arg("-c")
            .arg("import importlib.util, sys; print('found' if importlib.util.find_spec('glm_ocr') or importlib.util.find_spec('glmocr') else 'not found')")
            .output()
            .await;
        match check_import {
            Ok(out) => {
                let txt = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if txt.contains("found") && !txt.contains("not found") {
                    println!("  glm-ocr package: found");
                } else {
                    println!(
                        "  glm-ocr package: not found (pip install -r sidecar/requirements.txt)"
                    );
                }
                let err = String::from_utf8_lossy(&out.stderr);
                if !err.trim().is_empty() {
                    println!("  import stderr: {}", err.trim());
                }
            }
            Err(e) => {
                println!("  glm-ocr check failed: {e}");
            }
        }

        // Sidecar health
        println!("\n[sidecar]");
        let script = self
            .config
            .sidecar_path
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(find_sidecar_script);
        println!("  sidecar script: {}", script.display());
        println!("  exists: {}", if script.exists() { "yes" } else { "no" });
        let client = SidecarClient::from_config(&self.config.sidecar);
        println!("  is_available: {}", client.is_available());
        // Health check with timeout (quick)
        let health =
            tokio::time::timeout(std::time::Duration::from_secs(10), client.health_ok()).await;
        match health {
            Ok(true) => println!("  health: ok"),
            Ok(false) => println!("  health: degraded or not ok (may need model download, pip install -r sidecar/requirements.txt)"),
            Err(_) => println!("  health: timeout or error"),
        }
        println!(
            "  GLM status: {}",
            if client.is_available() {
                "available (if python+glm-ocr installed)"
            } else {
                "unavailable"
            }
        );

        // Shell
        println!("\n[shell]");
        #[cfg(target_os = "windows")]
        println!("  platform: windows");
        #[cfg(target_os = "linux")]
        println!("  platform: linux");
        #[cfg(target_os = "macos")]
        println!("  platform: macos");
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        println!("  platform: unknown");
        #[cfg(feature = "shell")]
        println!("  feature: shell enabled");
        #[cfg(not(feature = "shell"))]
        println!("  feature: shell disabled (build with --features shell)");

        // Check shell integration status
        let integration = crate::shell::current_integration();
        let installed = integration.is_installed();
        println!(
            "  shell integration: {}",
            if installed {
                "installed"
            } else {
                "not installed"
            }
        );
        if installed {
            println!(
                "    (use `txtify shell status` for details, `txtify shell uninstall` to remove)"
            );
        } else {
            println!("    (use `txtify shell install` to add context menu)");
        }

        // Check shell spawning
        let shell = crate::shell::current_shell();
        match shell.spawn_sidecar("echo", &["txtify"]) {
            Ok(mut child) => {
                let _ = child.wait();
                println!("  shell spawn: ok");
            }
            Err(e) => {
                println!("  shell spawn: error: {e}");
            }
        }

        // Platform-specific hints
        #[cfg(target_os = "windows")]
        {
            println!("  windows: HKCU\\Software\\Classes\\*\\shell\\Txtify");
            println!("  windows: HKCU\\Software\\Classes\\Directory\\shell\\Txtify (batch)");
        }
        #[cfg(target_os = "linux")]
        {
            let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
            println!("  linux: {home}/.local/share/file-manager/actions/txtify.desktop");
            println!("  linux: {home}/.local/share/nautilus/scripts/Txtify*");
        }
        #[cfg(target_os = "macos")]
        {
            println!("  macos: Finder Sync is Swift-only; use Automator Quick Action");
            println!("  macos: ~/Library/Services/Txtify.workflow");
        }

        println!("\ndoctor check complete.");
        Ok(())
    }

    async fn run_shell(&self, cmd: crate::cli::ShellCommands) -> Result<(), TxtifyError> {
        use crate::cli::ShellCommands;
        let integration = crate::shell::current_integration();
        match cmd {
            ShellCommands::Install => {
                println!("Installing shell integration...");
                integration.install()?;
                if integration.is_installed() {
                    println!("Shell integration installed.");
                } else {
                    // For macOS, is_installed may remain false until user manually creates workflow
                    println!("Shell integration step completed (check status).");
                }
                Ok(())
            }
            ShellCommands::Uninstall => {
                println!("Uninstalling shell integration...");
                integration.uninstall()?;
                if !integration.is_installed() {
                    println!("Shell integration uninstalled.");
                } else {
                    println!("Shell integration uninstall attempted; please check manually.");
                }
                Ok(())
            }
            ShellCommands::Status => {
                let installed = integration.is_installed();
                println!(
                    "Shell integration: {}",
                    if installed {
                        "installed"
                    } else {
                        "not installed"
                    }
                );
                #[cfg(target_os = "windows")]
                println!("  Registry: HKCU\\Software\\Classes\\*\\shell\\Txtify");
                #[cfg(target_os = "linux")]
                {
                    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
                    println!("  Actions: {home}/.local/share/file-manager/actions/txtify.desktop");
                    println!("  Nautilus: {home}/.local/share/nautilus/scripts/");
                }
                #[cfg(target_os = "macos")]
                println!("  Workflow: ~/Library/Services/Txtify.workflow");

                // Also show feature status
                #[cfg(feature = "shell")]
                println!("  Feature: shell enabled");
                #[cfg(not(feature = "shell"))]
                println!("  Feature: shell disabled");

                Ok(())
            }
        }
    }

    async fn run_config(&self) -> Result<(), TxtifyError> {
        println!("txtify config");
        println!("==============");
        println!("\n[paths]");
        for p in Config::candidate_paths() {
            let exists = p.exists();
            println!(
                "  {} {}",
                if exists { "[found]" } else { "[     ]" },
                p.display()
            );
        }
        if let Some(resolved) = Config::resolved_path() {
            println!("\nresolved: {}", resolved.display());
        }
        println!("\n[config json]");
        println!("{}", self.config.to_json()?);
        println!("\n[sidecar]");
        println!(
            "  python: {}",
            self.config
                .sidecar
                .python_path
                .clone()
                .unwrap_or_else(detect_python)
        );
        println!("  script: {}", find_sidecar_script().display());
        println!("  backend: {}", self.config.sidecar.glm_backend);
        println!("  model: {}", self.config.sidecar.glm_model);
        Ok(())
    }
}
