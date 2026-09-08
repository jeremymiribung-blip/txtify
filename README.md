# txtify

Hybrid document-to-text converter — **Fast** (pure Rust, ~42 ms) vs **High-Quality** (GLM-OCR 0.9B sidecar, 1.86 pages/s, 94.62 OmniDocBench).
Fast 42ms vs GLM 1.86 pg/s.

`txtify` converts `pdf`, `docx`, `xlsx`, `pptx`, `html`, `txt`, `md`, `csv`, and images (`png`/`jpg`/`tiff`) to `md`/`txt`/`json` using the best available engine for the file type and mode.

## Install

### Quick install (Linux/macOS) — curl (lädt alles inkl. KI automatisch)

```bash
# Inspect first, then pipe to sh (recommended)
curl -fsSL https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.sh -o install.sh
less install.sh
sh install.sh

# One-liner (after inspection) — lädt Binary + Python-Deps + KI-Modell (~1GB) + Config
curl -fsSL https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.sh | sh

# Nur Fast-Modus ohne KI-Download (schnell, kein ~1GB Download)
curl -fsSL https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.sh | sh -s -- --no-model

# Mit Rechtsklick-Menü
curl -fsSL https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.sh | sh -s -- --with-shell

# Specific version or prefix
curl -fsSL https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.sh | sh -s -- --version v0.1.0 --prefix ~/.local/bin
TXTIFY_VERSION=v0.1.0 sh installer/install.sh
TXTIFY_NO_MODEL=1 sh installer/install.sh   # dto. per Env, ohne KI-Modell
```

Der Installer erkennt `x86_64/aarch64` + `linux/musl` oder `apple-darwin`, lädt `txtify-<target>.tar.gz` von `releases/latest/download`, verifiziert `sha256`, installiert nach `~/.local/bin` (oder `/usr/local/bin`-Fallback), trägt `PATH` in `~/.bashrc`/`~/.zshrc` ein und führt danach automatisch `txtify setup --yes` aus: Python >= 3.10 prüfen, `pip install -r sidecar/requirements.txt`, KI-Modell `zai-org/GLM-OCR` (~1 GB) von Hugging Face prefetchen (Cache `~/.cache/huggingface/hub`), Default-Config anlegen. Mit `--no-setup` nur das Binary installieren (für CI/Docker), später mit `txtify setup --yes` nachholen. Siehe `sh installer/install.sh --help`.

### Quick install (Windows) — PowerShell (lädt alles inkl. KI automatisch)

```powershell
# Inspect first
Invoke-WebRequest -Uri https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.ps1 -OutFile install.ps1
Get-Content install.ps1 | More
.\install.ps1

# One-liner (after inspection) — lädt alles inkl. KI-Modell (~1GB)
irm https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.ps1 | iex

# Optionen
.\install.ps1 -Version v0.1.0 -Prefix "$env:LOCALAPPDATA\txtify" -Force
.\install.ps1 -NoModel        # nur Fast-Modus, ohne ~1GB Download
.\install.ps1 -WithShell      # auch Rechtsklick-Menü
.\install.ps1 -NoSetup        # nur Binary, Setup später: txtify setup --yes
```

Installiert `txtify.exe` nach `%LOCALAPPDATA%\txtify` (oder `-Prefix`), verifiziert `sha256`, trägt HKCU-`PATH` ein (kein Admin) und führt `txtify setup --yes` aus (Python-Deps + GLM-OCR-Modell + Config). Danach `txtify doctor` zur Kontrolle, `txtify shell install` falls nicht schon via `-WithShell` geschehen.

### `txtify setup` — alle Ressourcen nachladen (auch manuell)

```bash
txtify setup --yes                  # alles: Python-Deps + KI-Modell (~1GB) + Config
txtify setup --yes --no-model       # ohne Modell (nur Fast-Modus)
txtify setup --yes --with-shell     # inkl. Rechtsklick-Menü
txtify setup --check                # nur prüfen, nichts ändern
txtify setup --model zai-org/GLM-OCR --python /usr/bin/python3
txtify doctor                       # danach Systemcheck
```

### From crates.io (once published)

```bash
cargo install txtify --all-features
txtify --help
```

### From source

```bash
git clone https://github.com/jeremymiribung-blip/txtify
cd txtify
cargo build --release
# binary <15MB without sidecar
./target/release/txtify --help
# optional: install to PATH
cargo install --path . --all-features
```

### Windows installer (alternative)

Download `txtify-0.1.0-windows-x86_64-setup.exe` from Releases. Built via Inno Setup from `installer/windows/install.iss`:

- Installs `txtify.exe` to `%ProgramFiles%\Txtify` (HKCU, no admin required for shell integration)
- Optional: add to `PATH` (checkbox)
- Optional: sidecar files (`sidecar/txtify_sidecar.py`, `requirements.txt`) copied to `{app}\sidecar` if present
- Run `txtify doctor` after install to verify `pandoc`/`python`/`glm-ocr`

```bash
# silent install
txtify-*-setup.exe /SILENT
txtify doctor
```

### Sidecar prerequisites (only for `--mode high-quality`)

High-Quality mode uses the GLM-OCR 0.9B sidecar (`sidecar/`). No setup needed for Fast mode.

```bash
# Python 3.10+ required
python3 --version
pip install -r sidecar/requirements.txt  # glm-ocr, torch (cpu), transformers

# Verify
txtify doctor
# On first run, model zai-org/GLM-OCR (~1GB) is downloaded from Hugging Face to
# ~/.cache/huggingface/hub (see note below). Subsequent runs are instant (model cached).
```

## Usage CLI + Right-Click

Usage CLI + Right-Click — CLI and shell integration.

### CLI

```bash
# Single file → stdout (default fast)
txtify convert input.pdf --to md --mode fast
txtify convert input.docx -o out.md
txtify convert input.xlsx --to json --mode fast

# Multiple files → directory (parallel)
txtify convert a.pdf b.docx c.html -o out_dir/ --to md --mode fast

# Stdout dash
txtify convert a.pdf -o - --to md

# Overwrite / skip
txtify convert *.pdf -o out/ --overwrite
txtify convert *.pdf -o out/ --skip-existing

# Batch folder (recursive, progress bar)
txtify batch ./docs --to md --mode fast
txtify batch ./docs --recursive --to md --mode high-quality -o ./out --overwrite

# GLM-OCR backends
txtify convert scan.pdf --mode high-quality --backend auto        # default: vllm > sglang > transformers
txtify convert scan.pdf --mode high-quality --backend vllm
txtify convert scan.pdf --mode high-quality --backend transformers

# Other commands
txtify doctor   # full system check (see below)
txtify config   # show config paths + JSON
txtify --help
txtify convert --help
```

### Right-Click (shell integration)

txtify adds OS-native context menus (no admin on Windows). Install via CLI:

```bash
txtify shell install
txtify shell status
txtify shell uninstall
```

| Platform | What is installed | Location |
|----------|-------------------|----------|
| **Windows** | HKCU `Software\Classes\*\shell\Txtify` + `Directory\shell\Txtify` (3 subcommands) | Registry `HKCU\...` |
| **Linux** | `file-manager/actions/txtify.desktop` + `nautilus/scripts/Txtify*` + `kio/servicemenus` | `~/.local/share/...` |
| **macOS** | Automator Quick Action instructions (Finder Sync is Swift-only) | `~/Library/Services/Txtify.workflow` |

After `shell install`:

- **Windows**: right-click any file → *Txtify* → *Convert to Markdown (Fast / High Quality) / Convert to Text* — or right-click a folder → *Batch*
- **Linux**: right-click in Nautilus/Dolphin → *Txtify*
- **macOS**: create Automator Quick Action as printed by `txtify shell install` (uses `Run Shell Script`: `txtify convert "$f" --to md --mode fast`)

## Architecture diagram Fast vs GLM-OCR

Architecture diagram Fast vs GLM-OCR.

txtify splits conversion into two complementary paths. The detector picks the format, the registry picks the best converter for the requested mode.

```
                         ┌──────────────────────┐
                         │   CLI (clap)         │
                         │  src/cli/mod.rs      │
                         └──────────┬───────────┘
                                    │ Commands::Convert / Batch / Doctor / Shell
                         ┌──────────▼───────────┐
                         │  App                 │
                         │  src/app.rs          │  orchestrates, parallel via tokio::task::JoinSet
                         └──────────┬───────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              │                     │                     │
   ┌──────────▼──────────┐  ┌───────▼────────┐  ┌────────▼─────────┐
   │ Detector            │  │ Registry       │  │ Config           │
   │ src/core/detector.rs│  │ src/core/registry.rs│ src/config/mod.rs│
   │ extension + magic   │◄─┤  Fast → Pandoc │  │ txtify.toml,     │
   │ bytes (%PDF, PK,    │  │  → GLM-OCR*    │  │ sidecar_path,    │
   │  PNG/JPEG/TIFF)     │  │  *HighQuality │  │ backend/model    │
   └─────────────────────┘  └───────┬────────┘  └──────────────────┘
                                   │ Converter trait (async)
            ┌──────────────────────┼──────────────────────┐
            │                      │                      │
┌───────────▼─────────┐  ┌────────▼────────┐  ┌──────────▼──────────┐
│ FastConverter       │  │ PandocConverter │  │ GlmOcrConverter     │
│ src/converters/fast │  │ src/converters/ │  │ src/converters/     │
│ pure Rust, instant  │  │ pandoc.rs       │  │ sidecar/glm.rs      │
│ docx-rs, calamine,  │  │ pandoc -f <in>  │  │ SidecarClient       │
│ lopdf+pdf-extract,  │  │ -t gfm wrapper  │  │ src/shell/*.rs      │
│ html2md, encoding_rs│  │ fallback        │  │ JSON over stdio     │
└─────────────────────┘  └─────────────────┘  │ sidecar/txtify_     │
                                              │ sidecar.py (1.86    │
                                              │  pg/s, BF16, MTP)   │
                                              └─────────────────────┘
```

**Fast (pure Rust, instant)** — `src/converters/fast/`

- Engine: built-in parsers, no external processes.
- Formats: `pdf` (text-layer), `docx`, `xlsx`, `pptx`, `html`, `txt`/`md`/`csv`.
- Features: headings (`#` for docx Heading1, `## INTRODUCTION` for PDF all-caps), markdown tables, bold/italic, lists, sheet names, slide sections, encoding detection (BOM + windows-1252 fallback).
- Selection: always preferred in `--mode fast`; also used in `high-quality` when pandoc/glm unavailable? No — Fast only supports `ConversionMode::Fast`.

**High-Quality (GLM-OCR 0.9B sidecar)**

- Engine: `sidecar/txtify_sidecar.py` — lazy-loads `zai-org/GLM-OCR` via `GlmOcr` (or transformers fallback `AutoModel` with `trust_remote_code`), two-stage: `PP-DocLayout-V3` layout → parallel region recognition (thread pool), BF16, batch regions.
- Backend auto-detection: `GLM_ENGINE` env > `vllm` (if installed) > `sglang` > `OLLAMA_HOST` > `transformers` (CogViT 0.4B + GLM 0.5B, MTP enabled).
- Formats: `pdf`, `docx`, `pptx`, `xlsx`, `html`, `png`/`jpg`/`tiff` (scanned PDFs, complex layouts).
- Interface: `src/shell/{windows,linux,macos}.rs::Shell::spawn_sidecar` → `python sidecar/txtify_sidecar.py`, protocol `{"op":"convert","path":"...","to":"md","engine":"glm_ocr","backend":"auto"}` ↔ `{"markdown":"...","pages":3,"error":null}`, 600 s timeout (local CPU inference is slow), health check.

**Pandoc fallback** — `src/converters/pandoc.rs` — spawns `pandoc -f <from> -t gfm --wrap=none`, returns `ConversionFailed` with install hint if missing.

## Performance

| Path | Metric | Notes |
|------|--------|-------|
| **Fast (pure Rust)** | **~42 ms** per file (text/html/md) — **Fast 42ms** | Sub-100 ms for typical docs; no model load; measured via `cargo bench --bench conversion` (`benches/conversion.rs` criterion) |
| **High-Quality (GLM-OCR 0.9B)** | **1.86 pages/s** — **GLM 1.86 pg/s** | Two-stage PP-DocLayout-V3 + parallel region, BF16, batch; 94.62 OmniDocBench |
| Binary size (release, without sidecar) | **<15 MB** | `cargo build --release` with `opt-level=3`, `lto=true`, `strip=true`, `codegen-units=1` |

Run the bench:

```bash
cargo bench --bench conversion
# outputs benches/conversion.rs: text_passthrough, html_to_md, docx_to_md, xlsx_to_md, pdf_extract, pptx_to_md
```

## `txtify doctor` guide

`txtify doctor` checks all dependencies and prints actionable hints. It never panics.

```bash
txtify doctor
```

Example output (trimmed):

```
txtify doctor
=============

[config]
  /home/you/.config/txtify/config.toml : found
  resolved: /home/you/.config/txtify/config.toml
  config: { "mode": "Fast", "sidecar": { "glm_backend": "auto", "glm_model": "zai-org/GLM-OCR" } }

[pandoc]
  pandoc: available
  version: pandoc 3.1.8

[python]
  python_path: /home/you/.venv/bin/python3
  python: available (Python 3.11.8)

[glm-ocr]
  model: zai-org/GLM-OCR
  backend: auto
  glm-ocr package: not found (pip install -r sidecar/requirements.txt)

[sidecar]
  sidecar script: /home/you/txtify/sidecar/txtify_sidecar.py
  exists: yes
  is_available: true
  health: degraded or not ok (may need model download, pip install -r sidecar/requirements.txt)
  GLM status: available (if python+glm-ocr installed)

[shell]
  platform: linux
  feature: shell disabled (build with --features shell)
  shell integration: not installed
    (use `txtify shell install` to add context menu)
  shell spawn: ok
  linux: /home/you/.local/share/file-manager/actions/txtify.desktop

doctor check complete.
```

Fixing issues:

- `pandoc: not found` → `sudo apt install pandoc` / `brew install pandoc` / https://pandoc.org/installing.html
- `python: not found` → set `sidecar.python_path` in `txtify.toml` (`~/.config/txtify/config.toml` or `./txtify.toml`)
- `glm-ocr package: not found` → `pip install -r sidecar/requirements.txt` (`glm-ocr`, `torch`, `transformers`)
- `health: degraded` → first run triggers model download (~1 GB, see below); also check `pip install -r sidecar/requirements.txt`

Config example (`~/.config/txtify/config.toml`):

```toml
mode = "Fast"
sidecar_path = "sidecar/txtify_sidecar.py"
timeout_secs = 60

[sidecar]
python_path = "/home/you/.venv/bin/python3"
glm_backend = "auto"      # auto|transformers|vllm|sglang|ollama
glm_model = "zai-org/GLM-OCR"
```

## GLM-OCR first-run model download note (~1GB from huggingface)

GLM-OCR first-run model download note (~1GB from huggingface).

On the **first** `--mode high-quality` conversion (or `doctor` health check that probes the pipeline), the sidecar lazy-loads the model:

- **Model**: `zai-org/GLM-OCR` (≈ 0.9 B params: CogViT 0.4 B + GLM 0.5 B, MTP enabled)
- **Size**: ~**1 GB** (weights) + tokenizer/processor
- **Source**: `https://huggingface.co/zai-org/GLM-OCR` via `huggingface_hub` (used by `transformers`/`glm-ocr`)
- **Cache**: `~/.cache/huggingface/hub/models--zai-org--GLM-OCR/` (Linux/macOS) or `%USERPROFILE%\.cache\huggingface\hub\` (Windows). Subsequent runs hit cache — no re-download.
- **Time**: first init may take minutes depending on network (e.g., ~1 GB at 10 MB/s ≈ 100 s); convert timeout is 600 s (local CPU inference), 10 s for `doctor` — `doctor` may show `health: degraded` until model is cached.
- **Override**: `GLM_MODEL` env or `sidecar.glm_model` config changes the ID (e.g., local path). `GLM_BACKEND` / `sidecar.glm_backend` picks `transformers`/`vllm`/`sglang`/`ollama`.

If offline or download fails, the sidecar returns `{"markdown":null,"error":"... pip install -r sidecar/requirements.txt"}` and the Rust side maps it to `TxtifyError::SidecarNotFound` with hint.

## Project structure

```
src/
  main.rs          # tracing with EnvFilter, tokio::main
  lib.rs
  app.rs           # App::run, run_convert, run_batch, run_doctor, run_shell
  core/
    error.rs       # TxtifyError
    types.rs       # InputFormat, ImageFormat, OutputFormat, ConversionMode, GlmEngine, ConversionRequest/Result
    traits.rs      # Converter (async), FormatDetector
    registry.rs    # ConverterRegistry (Fast → Pandoc → GLM-OCR@HighQuality)
    detector.rs    # DefaultDetector (extension + magic bytes, proptest)
  converters/
    mod.rs
    fast/mod.rs    # FastConverter dispatch
    fast/docx.rs   # docx-rs → md (headings, tables, bold/italic, lists)
    fast/xlsx.rs   # calamine → md (sheets as tables)
    fast/pdf.rs    # lopdf + pdf-extract → md (page markers, scanned hint)
    fast/pptx.rs   # zip + quick-xml → md (slides)
    fast/html.rs   # html2md + encoding_rs
    fast/text.rs   # encoding_rs (BOM/windows-1252)
    sidecar/mod.rs # GlmOcrConverter, SidecarConverter
    sidecar/client.rs # SidecarClient (tokio::process::Command, 600s timeout, health_ok)
    sidecar/glm.rs    # GlmOcrConverter (HighQuality only)
    pandoc.rs      # PandocConverter (fallback)
  shell/
    mod.rs         # Shell, ShellIntegration, current_shell/integration
    windows.rs     # HKCU registry, mock FS for tests
    linux.rs       # .desktop + nautilus scripts + dolphin
    macos.rs       # Automator Quick Action instructions
  config/mod.rs    # Config, SidecarConfig (candidate_paths, from_json)
  cli/mod.rs       # Cli (clap derive), Mode, OutputFormatArg, Backend, Commands
sidecar/
  txtify_sidecar.py   # GLM-OCR 0.9B sidecar, JSON stdio, lazy model, 1.86 pg/s
  requirements.txt    # glm-ocr, torch, transformers
  README.md
tests/fixtures/       # sample.docx, sample.xlsx, sample.pdf, sample.html, sample.pptx, sample.txt, sample.png/jpg/tiff
tests/integration/
  fast_converters.rs  # heading/table/slide checks via fixtures
  glm_sidecar.rs      # health/protocol + unit without python
benches/
  conversion.rs       # criterion bench for fast converters (42 ms target)
installer/windows/install.iss # Inno Setup script
.github/workflows/ci.yml   # test on windows/ubuntu/macos + clippy + fmt + build --release
```

## Quality gates

```bash
cargo fmt --check          # enforced via rustfmt.toml (edition 2021, 100 cols)
cargo clippy --all-features -- -D warnings -W clippy::pedantic  # zero warnings (unwrap/expect/panic deny)
cargo test --all-features  # 83+ lib + 12 integration + proptest; docx/xlsx/pdf/image fixtures
cargo bench --bench conversion --no-run  # fast converters criterion bench
cargo doc --no-deps        # rustdoc for all pub APIs (///)
cargo build --release      # binary <15MB without sidecar (strip, lto)
```

- `rustfmt.toml` and `.clippy.toml` enforce style.
- `Cargo.toml [lints.clippy]`: `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"` — no `unwrap`/`expect` in `src/` (tests may use with context).
- `tracing` + `tracing-subscriber` with `EnvFilter` in `main.rs`.
- Coverage: `cargo llvm-cov --all-features` — **>80 % on `core`/`fast`** (82 % lines aggregated; `core/detector.rs` property test via `proptest`).
- `build.rs` embeds `sidecar/` (rerun-if-changed, env `TXTIFY_SIDECAR_PATH`).

## License

MIT OR Apache-2.0 — see `LICENSE` / `LICENSE-MIT` / `LICENSE-APACHE`.
