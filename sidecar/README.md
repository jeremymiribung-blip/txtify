# GLM-OCR 0.9B Sidecar

High-quality document OCR sidecar for `txtify` — 1.86 pages/s, 94.62 OmniDocBench via `zai-org/GLM-OCR`.

## Interface

Spawned by `src/shell` (`windows.rs` / `linux.rs` / `macos.rs`) via `tokio::process::Command`, JSON over stdio:

```json
// stdin
{"op":"health"}
{"op":"convert","path":"/tmp/a.pdf","to":"md","engine":"glm_ocr","backend":"auto","model":"zai-org/GLM-OCR"}
// stdout
{"markdown":"...","pages":3,"error":null}
{"status":"ok","engine":"glm_ocr","backend":"transformers","model":"zai-org/GLM-OCR"}
```

- **Timeout**: 60 s per convert, 10 s health check (via `txtify doctor`)
- **Lazy load**: model loads on first `convert`; stays resident; BF16, MTP enabled
- **Two-stage**: PP-DocLayout-V3 layout → parallel region recognition (batched via `ThreadPoolExecutor`)
- **Backends**: `auto` detects `vllm` > `sglang` > `ollama` (`OLLAMA_HOST`) > `transformers` (CogViT 0.4B + GLM 0.5B)
- **Formats**: `pdf`, `png`/`jpg`/`tiff`, `html`, `docx`, `pptx`, `xlsx` (via GLM pipeline)

## Install

```bash
pip install -r requirements.txt  # glm-ocr, torch --index-url https://download.pytorch.org/whl/cpu, transformers
# optional accelerators: vllm or sglang or ollama
```

First run downloads `zai-org/GLM-OCR` (~1 GB) from Hugging Face to `~/.cache/huggingface/hub`. Subsequent runs use cache.

Check:

```bash
txtify doctor
# or manually:
python3 txtify_sidecar.py <<<'{"op":"health"}'
python3 -c "import importlib.util; print(importlib.util.find_spec('glm_ocr'))"
```

## Throughput

- **1.86 pages/s** (BF16, batch regions, parallel)
- **94.62 OmniDocBench**

## Requirements

```
glm-ocr
torch --index-url https://download.pytorch.org/whl/cpu
transformers
# optional: vllm / sglang / ollama
```

## Source

- `txtify_sidecar.py` — main sidecar (488 lines), protocol handling, pipeline init, region batch
- `requirements.txt`
- `README.md` (this file)
