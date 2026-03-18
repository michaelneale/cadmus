# Cadmus vs pi — Local LLM Agent Eval

**Model**: `glm-4.7-flash:latest` (19GB, Ollama)  
**Hardware**: Apple M4 Max, 64GB RAM, macOS  
**Date**: 2026-03-06  

Both tools given identical tasks. Cadmus has a deterministic NL pipeline that handles known patterns without calling the LLM at all (**[NL]** = no LLM, **[LLM]** = model called).

---

## Results

| # | Task | Cadmus | pi | Notes |
|---|------|--------|-----|-------|
| 1 | `list files` | ✅ **0.3s** [NL] | ✅ 3.0s | Cadmus NL shortcut, no LLM needed |
| 2 | `create hello.txt with 'hello world'` | ✅ 3.3s | ✅ 4.0s | Both 1 tool call |
| 3 | `replace 'world' with 'cadmus' in hello.txt` | ✅ 4.7s | ✅ 4.0s | Cadmus read→edit→verify |
| 4 | `write fib.py + run it` | ✅ 8.1s | ✅ 10.0s | Both wrote + executed correctly |
| 5 | `mkdir scripts, move fib.py into it` | ✅ 5.5s | ✅ 5.0s | — |
| 6 | `HTML + separate JS file` | ✅ 11.6s | ✅* 10.2s | pi put `<script>` in `<head>` (DOM bug) |
| 7 | `system report → file` | ✅ 25.1s | ✅ 25.4s | pi's report more detailed |
| 8 | `top 5 CPU processes` | ✅ 21.7s | ✅ 23.4s | — |
| 9 | `hostname + local IP` | ✅ **5.1s** | ✅ 8.6s | Cadmus 1 step (macOS-native cmd) |
| 10 | `wifi network name` | ❌ timeout | ✅ 9.0s | Model doesn't know macOS wifi cmds |
| 11 | `open Calculator app` | ✅ **1.9s** | ✅ 5.7s | — |
| 12 | `battery status` | ✅ **2.8s** | ✅ 6.8s | Cadmus: `pmset` first try |
| 13 | `todo app (HTML + JS)` | ✅ **13.9s** | ✅ 26.7s | pi added delete + XSS protection |
| | **Total** | **12/13** | **13/13** | |

*Goose was also tested (same model) but scored **0/5** — glm-4.7-flash can't produce valid tool-call JSON for Goose's native format.*

---

## Where Cadmus Wins

**Speed on known patterns** — "list files" in 0.3s vs 3s. The NL pipeline recognises ~350 verbs, ~1450 synonyms, and ~280 plan templates without touching the LLM.

**Faster system commands** — OS detection in the prompt means the model reaches for `pmset`, `open -a`, `ipconfig` on the first try instead of burning 3-5 attempts on Linux equivalents. Battery: 2.8s vs 6.8s.

**Correct HTML patterns** — `<script>` placed at end of `<body>` so the DOM is ready. pi put it in `<head>`, breaking the click handler.

## Where pi Wins

**Richer output** — System report had per-volume disk breakdown, VM stats, and recommendations. Todo app got delete buttons and XSS escaping unprompted.

**100% pass rate** — pi handled everything including the wifi query that Cadmus timed out on. pi's broader tool set and model-agnostic architecture mean fewer edge cases.

**More resilient** — No false-positive risk from a pattern-matching layer. Every task goes through the LLM which understands context.

## Where Both Fail

**Wifi on glm-4.7-flash** — Even with the macOS hint, this model doesn't know `networksetup -getairportnetwork en0`. Cadmus tried 11 macOS commands before timing out. pi got lucky or has better model prompting. This is model quality, not tool quality.

---

## Architecture Comparison

|  | Cadmus | pi |
|--|--------|-----|
| **Approach** | NL pipeline first, LLM fallback | LLM always |
| **Tool protocol** | Text: `ACTION: tool(param="val")` | Native tool calling |
| **LLM cost per task** | 0 (NL hit) or 1+ calls | Always 1+ calls |
| **Latency floor** | ~0.3s (NL) | ~3s (minimum LLM round-trip) |
| **OS awareness** | Injected in system prompt | Built-in |
| **Risk** | NL false positives | Model hallucination |
| **Tool count** | 19 base + contextual domain ops | Full shell + file access |

---

## Reproduce

```bash
# Prerequisites
ollama pull glm-4.7-flash:latest
cargo build --release            # cadmus
npm i -g @mariozechner/pi-coding-agent  # pi

# Run all evals
./evals/run_evals.sh

# Or individual:
./target/release/cadmus --agent "list files"
pi --provider ollama --model "glm-4.7-flash:latest" -p "list files"
```
