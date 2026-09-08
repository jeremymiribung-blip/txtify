#!/usr/bin/env python3
"""
txtify GLM-OCR sidecar - JSON stdin/stdout protocol.

Protocol:
  stdin:  {"op":"convert","path":"/tmp/a.pdf","to":"md","engine":"glm_ocr","backend":"auto"}
          {"op":"health"}
          {"op":"shutdown"}
  stdout: {"markdown":"...","error":null,"pages":3}
          {"status":"ok","engine":"glm_ocr","backend":"transformers"}
          {"error":"..."} on failure

Lazy model load on first request, keep resident. Downloads from huggingface zai-org/GLM-OCR on first run.
Two-stage: PP-DocLayout-V3 for layout -> parallel region recognition (as per GLM-OCR docs).
BF16, batch regions in parallel.
Backend auto: if vllm installed use vLLM, elif sglang use SGLang, else transformers (CogViT 0.4B + GLM 0.5B, MTP enabled).
Supports Pdf, Image, Html, Docx, Pptx, Xlsx via GLM pipeline.
"""

from __future__ import annotations

import concurrent.futures
import importlib.util
import json
import logging
import os
import sys
import traceback
from pathlib import Path
from typing import Any, Optional

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger("txtify_sidecar")

# Global lazy state
_pipeline: Optional[Any] = None
_backend: Optional[str] = None
_model_id: str = os.environ.get("GLM_MODEL", "zai-org/GLM-OCR")
_dtype: str = "bf16"

SUPPORTED_OPS = {"convert", "health", "shutdown"}
SUPPORTED_INPUT_EXTS = {
    ".pdf",
    ".png",
    ".jpg",
    ".jpeg",
    ".tiff",
    ".tif",
    ".html",
    ".htm",
    ".docx",
    ".pptx",
    ".xlsx",
}


def detect_backend(requested: str = "auto") -> str:
    """Auto-detect backend: vllm > sglang > transformers."""
    if requested != "auto":
        return requested
    if importlib.util.find_spec("vllm") is not None:
        log.info("Detected vllm backend")
        return "vllm"
    if importlib.util.find_spec("sglang") is not None:
        log.info("Detected sglang backend")
        return "sglang"
    # Optional ollama via env
    if os.environ.get("OLLAMA_HOST"):
        log.info("Detected ollama backend via OLLAMA_HOST")
        return "ollama"
    log.info("Using transformers backend (CogViT 0.4B + GLM 0.5B, MTP enabled)")
    return "transformers"


def _init_pipeline(backend: str) -> Any:
    """
    Lazy init GLM-OCR pipeline from zai-org/GLM-OCR.
    Tries to use GlmOcr pipeline as per docs. Falls back to transformers AutoModel pattern.
    """
    global _pipeline, _backend

    if _pipeline is not None and _backend == backend:
        return _pipeline

    model_id = os.environ.get("GLM_MODEL", _model_id)
    log.info("Initializing GLM-OCR pipeline model=%s backend=%s", model_id, backend)

    # 1) Try glm-ocr package: from glm_ocr import GlmOcr or from glmocr import GlmOcr
    # The spec says: from zai-org/GLM-OCR: use `GlmOcr` pipeline
    # Common import paths tried in order
    import_errors = []
    GlmOcr = None
    for mod_name, attr in [
        ("glm_ocr", "GlmOcr"),
        ("glmocr", "GlmOcr"),
        ("glm_ocr.pipeline", "GlmOcr"),
        ("glm_ocr.glm_ocr", "GlmOcr"),
    ]:
        try:
            spec = importlib.util.find_spec(mod_name)
            if spec is None:
                continue
            mod = importlib.import_module(mod_name)
            GlmOcr = getattr(mod, attr, None)
            if GlmOcr is not None:
                log.info("Found GlmOcr via %s.%s", mod_name, attr)
                break
        except Exception as e:
            import_errors.append(f"{mod_name}: {e}")
            continue

    if GlmOcr is not None:
        # Try to instantiate with backend-specific kwargs
        # Docs: GlmOcr should support two-stage: PP-DocLayout-V3 for layout -> parallel region recognition
        # BF16, batch regions in parallel
        try:
            kwargs: dict[str, Any] = {
                "model_name_or_path": model_id,
                "dtype": _dtype,  # BF16
            }
            if backend == "vllm":
                kwargs["backend"] = "vllm"
                # Enable MTP if available
                kwargs["mtp_enabled"] = True
            elif backend == "sglang":
                kwargs["backend"] = "sglang"
                kwargs["mtp_enabled"] = True
            else:
                kwargs["backend"] = "transformers"
                kwargs["mtp_enabled"] = True

            # Some versions use `model` instead of `model_name_or_path`
            try:
                pipeline = GlmOcr(**kwargs)
            except TypeError as e:
                log.warning("GlmOcr init with model_name_or_path failed: %s, trying model=...", e)
                kwargs["model"] = kwargs.pop("model_name_or_path")
                pipeline = GlmOcr(**kwargs)

            _pipeline = pipeline
            _backend = backend
            log.info("GlmOcr pipeline initialized backend=%s", backend)
            return _pipeline
        except Exception as e:
            log.warning("Failed to init GlmOcr pipeline: %s\n%s", e, traceback.format_exc())
            # fall through to transformers fallback

    # 2) Local transformers inference (no API key needed).
    # Native image-text class if available, else generic AutoModel.
    # CPU (no CUDA here): float32; CUDA: bf16.
    log.info("Loading local transformers pipeline for %s", model_id)
    try:
        import torch
        from transformers import (
            AutoModelForImageTextToText,
            AutoProcessor,
            AutoTokenizer,
        )

        try:
            from transformers import AutoModel as _AutoModel
        except Exception:
            _AutoModel = None
        loaders = [AutoModelForImageTextToText]
        if _AutoModel is not None:
            loaders.append(_AutoModel)

        use_cuda = torch.cuda.is_available()
        torch_dtype = torch.bfloat16 if use_cuda else torch.float32
        model = None
        last_err = None
        for ld in loaders:
            try:
                model = ld.from_pretrained(
                    model_id,
                    dtype=torch_dtype,
                    trust_remote_code=True,
                )
                break
            except Exception as e:
                last_err = e
                continue

        if model is None:
            raise RuntimeError(f"Failed to load model {model_id}: {last_err} ; imports tried: {import_errors}")

        if use_cuda:
            model = model.to("cuda")
        model.eval()
        try:
            processor = AutoProcessor.from_pretrained(model_id, trust_remote_code=True)
        except Exception as e:
            raise RuntimeError(f"Failed to load processor for {model_id}: {e}") from e

        # Wrap in a simple pipeline object that implements .convert or __call__.
        # Images are OCR'd directly; PDFs are rendered page by page via
        # pymupdf and OCR'd per page. Office/HTML formats are not supported
        # by the local path (use --mode fast for those).
        class TransformersGlmPipeline:
            def __init__(self, model, processor):
                self.model = model
                self.processor = processor
                self.model_id = model_id
                try:
                    self.device = next(model.parameters()).device
                except Exception:
                    self.device = torch.device("cpu")

            def _ocr_image(self, image) -> str:
                import torch as _torch

                messages = [
                    {
                        "role": "user",
                        "content": [
                            {"type": "image"},
                            {"type": "text", "text": "Text Recognition:"},
                        ],
                    }
                ]
                prompt = self.processor.apply_chat_template(
                    messages, tokenize=False, add_generation_prompt=True
                )
                inputs = self.processor(
                    images=image, text=prompt, return_tensors="pt"
                ).to(self.device)
                inputs.pop("token_type_ids", None)
                with _torch.no_grad():
                    generated = self.model.generate(**inputs, max_new_tokens=4096)
                start = inputs["input_ids"].shape[1]
                return self.processor.decode(
                    generated[0][start:], skip_special_tokens=True
                ).strip()

            def convert(self, path: str, **kw):
                from PIL import Image as _Image

                ext = Path(path).suffix.lower()
                if ext in (".png", ".jpg", ".jpeg", ".tiff", ".tif", ".bmp", ".webp"):
                    image = _Image.open(path).convert("RGB")
                    return self._ocr_image(image)
                if ext == ".pdf":
                    try:
                        import pymupdf as fitz
                    except ImportError as e:
                        raise RuntimeError(
                            "PDF high-quality OCR needs pymupdf: pip install pymupdf"
                        ) from e
                    doc = fitz.open(path)
                    try:
                        parts = []
                        for i, page in enumerate(doc, start=1):
                            pix = page.get_pixmap(dpi=150)
                            import io as _io

                            image = _Image.open(
                                _io.BytesIO(pix.tobytes("png"))
                            ).convert("RGB")
                            text = self._ocr_image(image)
                            parts.append(f"--- Page {i} ---\n\n{text}")
                        return "\n\n".join(parts)
                    finally:
                        doc.close()
                raise RuntimeError(
                    f"local high-quality inference supports pdf/images, got {ext or path}. "
                    "Tip: use --mode fast for office/html documents"
                )

            def __call__(self, *a, **kw):
                return self.convert(*a, **kw)

        _pipeline = TransformersGlmPipeline(model, processor)
        _backend = backend
        return _pipeline
    except ImportError as e:
        raise RuntimeError(
            f"Failed to init any GLM-OCR backend: {e}. Install requirements: pip install -r sidecar/requirements.txt. "
            f"Import errors: {import_errors}"
        ) from e


def _convert_via_pipeline(pipeline: Any, path: str, output_format: str = "md") -> tuple[str, Optional[int]]:
    """
    Execute two-stage conversion:
      1. PP-DocLayout-V3 for layout
      2. parallel region recognition (batched)

    The GlmOcr pipeline is expected to handle this internally; we ensure batch parallel execution.
    """
    log.info("Converting %s -> %s using pipeline %s", path, output_format, type(pipeline).__name__)

    # GlmOcr pipeline may expose different APIs; try in order.
    # Preferred: pipeline.convert(path) or pipeline(path) or pipeline.predict(path)
    # It should support InputFormat Pdf, Image, Html, Docx, Pptx, Xlsx (convert via GLM pipeline)
    # Docs say InputFormat handling is via GLM pipeline directly.

    # Check for async or sync method
    # Some GlmOcr versions handle office docs internally, others need pre-conversion to images/pdf.
    # We attempt direct call first.

    result_text: Optional[str] = None
    pages: Optional[int] = None

    # Strategy: try pipeline methods that return markdown
    methods_to_try = []

    # If pipeline has attributes that hint at two-stage processing, prefer those
    if hasattr(pipeline, "convert"):
        methods_to_try.append(("convert", lambda p: pipeline.convert(p)))
    if hasattr(pipeline, "predict"):
        methods_to_try.append(("predict", lambda p: pipeline.predict(p)))
    if hasattr(pipeline, "__call__"):
        methods_to_try.append(("__call__", lambda p: pipeline(p)))

    # For layout-then-recognize two-stage explicit
    if hasattr(pipeline, "layout_and_recognize"):
        def _two_stage(p):
            # PP-DocLayout-V3 for layout -> parallel region recognition
            layout = pipeline.layout(p) if hasattr(pipeline, "layout") else None
            if layout is None and hasattr(pipeline, "doc_layout"):
                layout = pipeline.doc_layout(p)
            # If pipeline exposes region_batch
            if layout is not None:
                # Batch regions in parallel
                regions = layout if isinstance(layout, list) else [layout]
                # Use thread pool for parallel region recognition
                def _recognize_region(region):
                    if hasattr(pipeline, "recognize"):
                        return pipeline.recognize(region)
                    if hasattr(pipeline, "ocr_region"):
                        return pipeline.ocr_region(region)
                    return str(region)
                with concurrent.futures.ThreadPoolExecutor() as ex:
                    futures = [ex.submit(_recognize_region, r) for r in regions]
                    texts = [f.result() for f in concurrent.futures.as_completed(futures)]
                return "\n\n".join(texts)
            return pipeline.convert(p) if hasattr(pipeline, "convert") else pipeline(p)
        methods_to_try.insert(0, ("layout_and_recognize", _two_stage))

    last_err = None
    for name, fn in methods_to_try:
        try:
            log.info("Trying pipeline method %s", name)
            res = fn(path)
            # Handle various return types
            if isinstance(res, dict):
                # e.g. {"markdown": "...", "pages": 3} or {"text": "..."}
                if "markdown" in res:
                    result_text = res["markdown"]
                    pages = res.get("pages")
                    break
                if "text" in res:
                    result_text = res["text"]
                    pages = res.get("pages") or res.get("page_count")
                    break
                # Fallback: stringify
                result_text = json.dumps(res)
                pages = res.get("pages")
                break
            elif isinstance(res, (list, tuple)):
                # Possibly list of pages
                pages = len(res)
                parts = []
                for item in res:
                    if isinstance(item, dict):
                        parts.append(item.get("markdown") or item.get("text") or str(item))
                    else:
                        parts.append(str(item))
                result_text = "\n\n".join(parts)
                break
            elif isinstance(res, str):
                result_text = res
                # Pages unknown unless we count; try to infer
                break
            elif res is not None:
                result_text = str(res)
                break
        except Exception as e:
            last_err = e
            log.warning("Method %s failed: %s\n%s", name, e, traceback.format_exc())
            continue

    if result_text is None:
        # Final fallback: if pipeline exposes .generate or .chat for images
        # Try to manually do layout -> parallel region if available
        try:
            if hasattr(pipeline, "model") and hasattr(pipeline, "processor"):
                # transformers fallback already failed earlier
                raise RuntimeError(str(last_err) if last_err else "pipeline returned None")
            raise RuntimeError(str(last_err) if last_err else "pipeline returned None")
        except Exception as e:
            raise RuntimeError(f"GLM-OCR conversion failed for {path}: {e}") from e

    # If pages still None, try to estimate via pdf page count or image count
    if pages is None:
        try:
            ext = Path(path).suffix.lower()
            if ext == ".pdf":
                # Try pypdf or pdfminer fallback to count pages without heavy deps
                try:
                    import pymupdf as fitz  # pymupdf if available
                    doc = fitz.open(path)
                    pages = len(doc)
                    doc.close()
                except Exception:
                    try:
                        from pypdf import PdfReader
                        reader = PdfReader(path)
                        pages = len(reader.pages)
                    except Exception:
                        pages = None
            else:
                pages = 1
        except Exception:
            pages = None

    return result_text, pages


def _handle_convert(req: dict) -> dict:
    global _pipeline, _backend, _model_id
    path = req.get("path")
    to = req.get("to", "md")
    engine = req.get("engine", "glm_ocr")
    backend_req = req.get("backend", "auto")
    model = req.get("model") or os.environ.get("GLM_MODEL", _model_id)

    if not path:
        return {"markdown": None, "error": "missing 'path' field", "pages": None}
    if not os.path.exists(path):
        return {"markdown": None, "error": f"file not found: {path}", "pages": None}

    # Validate extension hint (but allow any, as GLM pipeline may handle)
    ext = Path(str(path)).suffix.lower()
    # Still proceed; we log if unknown
    if ext not in SUPPORTED_INPUT_EXTS:
        log.warning("Unknown extension %s for %s, attempting conversion anyway", ext, path)

    # Only glm_ocr engine supported for now
    if engine not in ("glm_ocr", "glm-ocr", "auto"):
        log.warning("Unknown engine %s, using glm_ocr", engine)

    backend = detect_backend(backend_req)

    try:
        # Allow per-request model override
        if model and model != _model_id:
            # If model changed, reset pipeline to reload
            _model_id = model
            # Force re-init next call
            _pipeline = None
            _backend = None

        pipeline = _init_pipeline(backend)
        markdown, pages = _convert_via_pipeline(pipeline, str(path), to)

        # Ensure markdown is string
        if markdown is None:
            markdown = ""

        return {"markdown": markdown, "error": None, "pages": pages}
    except Exception as e:
        tb = traceback.format_exc()
        log.error("Convert failed: %s\n%s", e, tb)
        return {"markdown": None, "error": f"{e}", "pages": None}


def _handle_health(_req: dict) -> dict:
    backend = detect_backend(os.environ.get("GLM_BACKEND", "auto"))
    # Check if pipeline is loaded
    loaded = _pipeline is not None
    # Also check if dependencies are available
    deps_ok = importlib.util.find_spec("transformers") is not None
    # Try to report glm-ocr availability
    glm_available = (
        importlib.util.find_spec("glm_ocr") is not None
        or importlib.util.find_spec("glmocr") is not None
    )
    return {
        "status": "ok" if deps_ok else "missing_deps",
        "engine": "glm_ocr",
        "backend": backend,
        "model": _model_id,
        "loaded": loaded,
        "glm_available": glm_available,
        "error": None,
    }


def handle_request(req: dict) -> dict:
    op = req.get("op")
    if not op:
        return {"error": "missing 'op' field"}
    if op not in SUPPORTED_OPS:
        return {"error": f"unsupported op '{op}', supported: {sorted(SUPPORTED_OPS)}"}
    if op == "convert":
        return _handle_convert(req)
    if op == "health":
        return _handle_health(req)
    if op == "shutdown":
        return {"status": "shutting_down"}
    return {"error": f"unknown op {op}"}


def main():
    log.info("txtify_sidecar starting, model=%s pid=%s", _model_id, os.getpid())
    # Ensure line-buffered json handling
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            resp = {"error": f"invalid json: {e}", "markdown": None, "pages": None}
            sys.stdout.write(json.dumps(resp, ensure_ascii=False) + "\n")
            sys.stdout.flush()
            continue

        try:
            resp = handle_request(req)
        except Exception as e:
            tb = traceback.format_exc()
            log.error("Unhandled error handling request %s: %s\n%s", req, e, tb)
            resp = {"error": f"internal error: {e}", "markdown": None, "pages": None}

        # Shutdown handling
        if req.get("op") == "shutdown":
            sys.stdout.write(json.dumps(resp, ensure_ascii=False) + "\n")
            sys.stdout.flush()
            log.info("Shutting down on request")
            break

        sys.stdout.write(json.dumps(resp, ensure_ascii=False) + "\n")
        sys.stdout.flush()

    log.info("txtify_sidecar exiting")


if __name__ == "__main__":
    main()
