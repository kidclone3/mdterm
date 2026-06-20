#!/usr/bin/env python3
"""Benchmark Z.AI Coding Plan models for response speed.

Measures, per model:
  - TTFB    : time to first token (best proxy for "fastest response")
  - total   : wall-clock time for the full response
  - tokens  : approximate output tokens (chars / 4)
  - tok/s   : output tokens per second (throughput)
  - retries : number of transient retries

Reads the API key from opencode's auth.json (provider: zai-coding-plan).
Uses only the Python standard library.

Usage:
  python3 bench_zai_models.py
  python3 bench_zai_models.py --iterations 3 --max-tokens 512
  python3 bench_zai_models.py --models glm-5.2 glm-5-turbo glm-4.5-air
  python3 bench_zai_models.py --json        # machine-readable output
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed

AUTH_PATH = os.path.expanduser("~/.local/share/opencode/auth.json")
BASE_URL = "https://api.z.ai/api/paas/v4"
PROVIDER = "zai-coding-plan"
DEFAULT_PROMPT = (
    "In Python, write a function `flatten(nested)` that flattens an "
    "arbitrarily nested list of ints. Include type hints, a docstring, "
    "and 3 doctest examples."
)
TRANSIENT_STATUS = {408, 425, 429, 500, 502, 503, 504}


def get_key() -> str:
    try:
        with open(AUTH_PATH) as f:
            data = json.load(f)
    except FileNotFoundError:
        sys.exit(f"auth.json not found at {AUTH_PATH}")
    if PROVIDER not in data or "key" not in data[PROVIDER]:
        sys.exit(f"provider '{PROVIDER}' not found in {AUTH_PATH}")
    return data[PROVIDER]["key"]


def list_models(key: str) -> list[str]:
    req = urllib.request.Request(
        f"{BASE_URL}/models", headers={"Authorization": f"Bearer {key}"}
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            payload = json.load(resp)
    except urllib.error.HTTPError as e:
        sys.exit(f"failed to list models (HTTP {e.code}): {e.read().decode('utf-8', 'replace')[:200]}")
    return sorted(m["id"] for m in payload.get("data", []))


def bench_one(key: str, model: str, prompt: str, max_tokens: int,
              timeout: int, retries: int) -> dict:
    """Stream a single completion and return timing stats."""
    payload = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": True,
        "max_tokens": max_tokens,
        "temperature": 0.0,
    }
    body = json.dumps(payload).encode("utf-8")

    attempts = 0
    last_err = None
    while attempts <= retries:
        attempts += 1
        req = urllib.request.Request(
            f"{BASE_URL}/chat/completions",
            data=body,
            headers={
                "Authorization": f"Bearer {key}",
                "Content-Type": "application/json",
                "Accept": "text/event-stream",
            },
            method="POST",
        )
        t0 = time.perf_counter()
        ttfb = None
        chars = 0
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                for raw in resp:
                    line = raw.decode("utf-8", "replace").strip()
                    if not line or not line.startswith("data:"):
                        continue
                    data = line[len("data:"):].strip()
                    if data == "[DONE]":
                        break
                    try:
                        chunk = json.loads(data)
                    except json.JSONDecodeError:
                        continue
                    if ttfb is None:
                        ttfb = time.perf_counter() - t0
                    choices = chunk.get("choices") or [{}]
                    delta = choices[0].get("delta", {}) or {}
                    chars += len(delta.get("content") or "")
            total = time.perf_counter() - t0
            if ttfb is None:
                last_err = "no tokens received (empty stream)"
                continue
            tokens_approx = max(1, chars // 4)
            return {
                "model": model,
                "ttfb": ttfb,
                "total": total,
                "tokens": tokens_approx,
                "tok_per_s": tokens_approx / total if total > 0 else 0.0,
                "retries": attempts - 1,
                "error": None,
            }
        except urllib.error.HTTPError as e:
            last_err = f"HTTP {e.code}: {e.read().decode('utf-8', 'replace')[:160]}"
            if e.code in TRANSIENT_STATUS and attempts <= retries:
                time.sleep(min(2 ** attempts, 8))
                continue
            break
        except Exception as e:  # noqa: BLE001
            last_err = f"{type(e).__name__}: {e}"
            break

    return {"model": model, "error": last_err or "unknown error", "retries": attempts - 1}


def fmt_ms(v) -> str:
    return f"{v * 1000:7.0f} ms" if isinstance(v, (int, float)) else "      n/a"


def run(args) -> int:
    key = get_key()
    available = list_models(key)
    models = args.models or available
    unknown = [m for m in models if m not in available]
    if unknown:
        print(f"warning: not in /models listing (will still try): {', '.join(unknown)}",
              file=sys.stderr)

    print(f"Endpoint : {BASE_URL}/chat/completions", file=sys.stderr)
    print(f"Models   : {', '.join(models)}", file=sys.stderr)
    print(f"Iterations: {args.iterations}  | max_tokens: {args.max_tokens}  | "
          f"concurrency: {args.concurrency}", file=sys.stderr)
    print(f"Prompt   : {args.prompt[:80]}{'...' if len(args.prompt) > 80 else ''}",
          file=sys.stderr)
    print("-" * 72, file=sys.stderr)

    # Each (model, iteration) pair is one task; models run concurrently up to limit.
    tasks = [(m, i) for m in models for i in range(args.iterations)]
    results: dict[str, list[dict]] = {m: [] for m in models}

    with ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futures = {
            pool.submit(bench_one, key, m, args.prompt, args.max_tokens,
                        args.timeout, args.retries): m
            for (m, _i) in tasks
        }
        for fut in as_completed(futures):
            m = futures[fut]
            res = fut.result()
            results[m].append(res)
            if res.get("error"):
                print(f"  [{m}] ERROR: {res['error']}", file=sys.stderr)
            else:
                print(f"  [{m}] ttfb={fmt_ms(res['ttfb'])} total={fmt_ms(res['total'])} "
                      f"~{res['tokens']} tok @ {res['tok_per_s']:.1f} tok/s",
                      file=sys.stderr)

    # Aggregate per model.
    summary = []
    for m, runs in results.items():
        ok = [r for r in runs if r.get("error") is None]
        if not ok:
            summary.append({"model": m, "error": runs[-1].get("error", "all failed")})
            continue
        ttfbs = [r["ttfb"] for r in ok]
        totals = [r["total"] for r in ok]
        summary.append({
            "model": m,
            "n": len(ok),
            "ttfb_min": min(ttfbs),
            "ttfb_avg": statistics.mean(ttfbs),
            "ttfb_median": statistics.median(ttfbs),
            "total_avg": statistics.mean(totals),
            "tok_per_s_avg": statistics.mean([r["tok_per_s"] for r in ok]),
            "retries_total": sum(r["retries"] for r in ok),
        })

    if args.json:
        print(json.dumps(summary, indent=2))
        return 0

    ok_rows = [s for s in summary if "error" not in s]
    err_rows = [s for s in summary if "error" in s]
    ok_rows.sort(key=lambda s: s["ttfb_median"])

    print("\n" + "=" * 72, file=sys.stderr)
    print("RESULTS (sorted by median TTFB — fastest first)", file=sys.stderr)
    print("=" * 72, file=sys.stderr)
    header = f"{'MODEL':<16}{'TTFB min':>11}{'TTFB med':>11}{'TTFB avg':>11}{'total avg':>12}{'tok/s':>9}"
    print(header, file=sys.stderr)
    print("-" * len(header), file=sys.stderr)
    for s in ok_rows:
        print(f"{s['model']:<16}{fmt_ms(s['ttfb_min']):>11}{fmt_ms(s['ttfb_median']):>11}"
              f"{fmt_ms(s['ttfb_avg']):>11}{fmt_ms(s['total_avg']):>12}"
              f"{s['tok_per_s_avg']:>8.1f}t/s", file=sys.stderr)
    for s in err_rows:
        print(f"{s['model']:<16}  ERROR: {s['error']}", file=sys.stderr)

    if ok_rows:
        winner = ok_rows[0]
        print(f"\nFastest (lowest median TTFB): {winner['model']}",
              file=sys.stderr)
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--models", nargs="*", default=None,
                   help="subset of models to test (default: all from /models)")
    p.add_argument("--iterations", type=int, default=2,
                   help="runs per model (results averaged; default: 2)")
    p.add_argument("--max-tokens", type=int, default=256,
                   help="max output tokens per call (default: 256)")
    p.add_argument("--concurrency", type=int, default=4,
                   help="parallel in-flight requests (default: 4)")
    p.add_argument("--timeout", type=int, default=120,
                   help="per-request timeout in seconds (default: 120)")
    p.add_argument("--retries", type=int, default=2,
                   help="retries on transient HTTP errors (default: 2)")
    p.add_argument("--prompt", default=DEFAULT_PROMPT,
                   help="prompt to send to each model")
    p.add_argument("--json", action="store_true",
                   help="print machine-readable JSON to stdout")
    args = p.parse_args()
    if args.iterations < 1:
        p.error("--iterations must be >= 1")
    if args.concurrency < 1:
        p.error("--concurrency must be >= 1")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
