# Fine-Tuning Plan for Cadmus Agent Model

> **Goal**: Fine-tune a small model that knows cadmus's 600 ops, 345 plans, and domain structure so `cadmus --agent` gets accurate tool selection and parameter formatting without a bloated system prompt.

## Problem

`cadmus --agent` currently uses a generic chat model (GLM-4.7-Flash) that has never seen any cadmus op. The system prompt dumps a tool catalog and the model guesses. This causes:

- **Wrong tool selection**: "list containers" → `list_dir` instead of `docker_ps`
- **Wrong parameters**: `directory="current"` instead of `dir="."`
- **No chaining knowledge**: doesn't know that PR review = `gh_pr_view` → `gh_pr_diff` → `gh_pr_checks`
- **Wasted context**: ~60 lines of tool descriptions eat into the model's context window

A fine-tuned model has cadmus ops in its weights. The system prompt shrinks to near-nothing. Tool selection becomes fast and accurate.

## What We're Training

The model learns three things:

1. **Tool selection**: "list running containers" → `docker_ps()`
2. **Parameter binding**: "search for TODO in src/" → `grep_code(dir="src/", pattern="TODO")`
3. **Multi-step sequences**: "review PR 42" → `gh_pr_view(pr_number="42")` then `gh_pr_diff(pr_number="42")` then `gh_pr_checks(pr_number="42")`

## Training Data (~8K examples from 4 sources)

### Source A: Op Registry → Single-Turn Tool Calls (~2,400)

For each of the 600 ops, generate 3-4 paraphrased instruction → ACTION pairs using the NL lexicon's verb synonyms.

```jsonl
{"messages":[{"role":"user","content":"List running docker containers"},{"role":"assistant","content":"ACTION: docker_ps()"}]}
{"messages":[{"role":"user","content":"Show me all containers"},{"role":"assistant","content":"ACTION: docker_ps()"}]}
{"messages":[{"role":"user","content":"What containers are up?"},{"role":"assistant","content":"ACTION: docker_ps()"}]}
```

Input data: `data/packs/ops/*.ops.yaml` (op names, descriptions, input_names, signatures), `data/nl/nl_lexicon.yaml` (verb synonyms for paraphrasing).

### Source B: Plan Files → Multi-Step Traces (~800)

For each of the 345 plans, extract the NL description and op sequence, format as multi-turn conversations with synthetic RESULT outputs.

```jsonl
{"messages":[
  {"role":"user","content":"Review pull request 42"},
  {"role":"assistant","content":"I'll review this PR.\nACTION: gh_pr_view(dir=\".\", pr_number=\"42\")"},
  {"role":"user","content":"RESULT: PR #42: Fix memory leak..."},
  {"role":"assistant","content":"Checking the diff.\nACTION: gh_pr_diff(dir=\".\", pr_number=\"42\")"},
  {"role":"user","content":"RESULT: +++ b/src/cache.rs..."},
  {"role":"assistant","content":"PR #42 looks good."}
]}
```

Input data: `data/plans/*.sexp` (plan definitions with descriptions and op sequences).

### Source C: Session Traces → Real User Workflows (~5,000)

Extract (user_message → tool_sequence) pairs from 94K real tool calls across 410 Pi sessions and 7,855 Goose sessions. Map session tool calls to cadmus ops:

| Session Tool | Cadmus Op |
|---|---|
| `bash("grep -r ...")` | `grep_code(dir=..., pattern=...)` |
| `bash("docker ps")` | `docker_ps()` |
| `bash("git log ...")` | `git_log(dir=...)` |
| `read(path=...)` | `read_file(file=...)` |
| `edit(path=..., old=..., new=...)` | `sed_replace(file=..., find=..., replace=...)` |

Input data: `~/.pi/agent/sessions/**/*.jsonl` (43K tool calls), `~/.local/share/goose/sessions/sessions.db` (50K tool calls).

### Source D: Negative Examples / DPO Pairs (~300)

Wrong-tool examples as rejected responses for preference training:

```jsonl
{"prompt":"List docker containers","chosen":"ACTION: docker_ps()","rejected":"ACTION: list_dir(path=\".\")"}
```

### Total

| Source | Examples |
|--------|---------|
| Op registry (augmented) | ~2,400 |
| Plan files (augmented) | ~800 |
| Session traces (filtered) | ~5,000 |
| DPO pairs | ~300 |
| **Total** | **~8,500** |

## Hardware

- Apple M4 Max, 64GB unified memory, 40 GPU cores, Metal 4
- Ollama 0.12.1, llama.cpp (brew, includes `llama-finetune`, `llama-quantize`, `llama-export-lora`)
- Python 3.11 + 3.13 via Homebrew, `uv` package manager
- MLX / mlx-lm: **not yet installed** (one `uv pip install`)

## Models on Disk

| Model | File | Size | Type |
|---|---|---|---|
| Qwen3-0.6B | `~/.models/Qwen3-0.6B-Q4_K_M.gguf` | 0.4GB | Dense, validation target |
| Qwen3-8B | `~/.models/Qwen3-8B-Q4_K_M.gguf` | 4.7GB | Dense, primary target |
| Qwen3-30B-A3B | `~/.models/Qwen3-30B-A3B-Q4_K_M.gguf` | 17GB | MoE (128 experts, top-8), stretch target |
| OLMoE-1B-7B | `~/.models/olmoe-1b-7b-0924-instruct-q4_k_m.gguf` | 3.9GB | MoE (64 experts, top-8), alternative |
| GLM-4.7-Flash | Ollama `glm-4.7-flash:latest` | 19GB | Current agent default |

HuggingFace cache has Qwen3-30B-A3B config.json (metadata only, not full weights — those are ~60GB BF16 and would need downloading for MLX LoRA).

## Training Approach

Two paths, try both:

### Path 1: `llama-finetune` on GGUF (fast validation)

Already installed. Trains directly from quantized GGUF files. Full parameter update (not LoRA). Plain text input formatted with the model's chat template.

```bash
# Format training data as plain text with Qwen chat template
python3 scripts/gen_training_data.py \
    --format llama-finetune \
    --output data/training/train.txt

# Fine-tune Qwen3-0.6B (~30 min on M4 Max)
llama-finetune \
    -m ~/.models/Qwen3-0.6B-Q4_K_M.gguf \
    -f data/training/train.txt \
    -o ~/.models/cadmus-0.6b-v1.gguf \
    --epochs 2 \
    --learning-rate 1e-5 \
    --batch-size 8

# Test
ollama create cadmus-0.6b -f Modelfile.0.6b
CADMUS_MODEL=cadmus-0.6b cadmus --agent "list docker containers"
```

**Use for**: Quick validation that training data is good. 30 min turnaround. If the 0.6B model picks correct tools >80% of the time, the data is working.

**Limitations**: Full fine-tune risks catastrophic forgetting. No LoRA. Only works with models we have as GGUF.

### Path 2: MLX LoRA (production quality)

Apple's native ML framework. LoRA fine-tuning preserves base model knowledge. Can target specific layers.

```bash
# One-time setup
uv venv ~/.venvs/cadmus-train --python 3.11
source ~/.venvs/cadmus-train/bin/activate
uv pip install mlx mlx-lm

# Format training data as JSONL
python3 scripts/gen_training_data.py \
    --format mlx \
    --output data/training/

# LoRA on Qwen3-8B (~2-4 hours)
mlx_lm.lora \
    --model Qwen/Qwen3-8B \
    --train \
    --data data/training/ \
    --adapter-path adapters/cadmus-8b-v1/ \
    --lora-layers 16 \
    --batch-size 1 \
    --iters 2000 \
    --learning-rate 2e-5

# Merge + convert to GGUF
mlx_lm.fuse --model Qwen/Qwen3-8B --adapter-path adapters/cadmus-8b-v1/ --output-path merged/
# Convert to GGUF via llama.cpp tools
```

**Use for**: The real production model. LoRA means minimal forgetting. Qwen3-8B has enough capacity for 600 ops across 8 domains.

**For MoE (Qwen3-30B-A3B)**: Same process but needs ~60GB safetensors download. The 30B MoE gives 10x parameter capacity at the same 3B inference cost. Whether that matters for 8K training examples is an empirical question — run both and compare.

## Memory Budget

| Model | Method | Estimated RAM | Fits 64GB? |
|---|---|---|---|
| Qwen3-0.6B | Full FT (GGUF) | ~2GB | ✓ easily |
| Qwen3-8B | LoRA 4-bit (MLX) | ~9GB | ✓ |
| Qwen3-30B-A3B | LoRA 4-bit (MLX) | ~25GB | ✓ |
| Qwen3-30B-A3B | LoRA BF16 (MLX) | ~65GB | ✗ (need 4-bit) |

## Evaluation

Run each model against a fixed eval set:

| Metric | How | Baseline (GLM untuned) | Target |
|---|---|---|---|
| Single-op accuracy | 200 prompts → correct op name? | ~60% | >90% |
| Param accuracy | 200 prompts → correct param names + values? | ~50% | >85% |
| Multi-step ordering | 50 plan traces → correct op sequence? | ~30% | >70% |
| System prompt size | Token count of system prompt needed | ~800 tokens | <200 tokens |

The eval set will be held out from training data (10% split).

## Deliverables

| # | Deliverable | Description |
|---|---|---|
| 1 | `scripts/gen_training_data.py` | Extracts training pairs from ops, plans, sessions. Outputs JSONL + plain text. |
| 2 | `data/training/` | Generated training data (~8K examples). |
| 3 | `scripts/eval_agent.py` | Runs eval prompts against a model endpoint, scores accuracy. |
| 4 | `Modelfile.cadmus` | Ollama Modelfile for the fine-tuned model. |
| 5 | Fine-tuned model GGUF | Published to `~/.models/cadmus-*.gguf`. |

## Execution Order

| Step | What | Time | Depends On |
|---|---|---|---|
| 1 | Write `gen_training_data.py` | 1 day | — |
| 2 | Generate training data, inspect quality | 2 hours | Step 1 |
| 3 | `llama-finetune` on Qwen3-0.6B (validation) | 30 min | Step 2 |
| 4 | Evaluate 0.6B model, iterate on data if needed | 2 hours | Step 3 |
| 5 | Install MLX, LoRA on Qwen3-8B | 4 hours | Step 2 |
| 6 | Evaluate 8B model | 2 hours | Step 5 |
| 7 | (Optional) LoRA on Qwen3-30B-A3B MoE | 8 hours | Step 2, download 60GB weights |
| 8 | Compare all models, pick winner | 2 hours | Steps 4, 6, (7) |
| 9 | Deploy: Modelfile + update agent default | 1 hour | Step 8 |

## Open Questions

1. **Is 8K examples enough?** The 0.6B validation run (Step 3) will tell us. If it memorizes but can't generalize to paraphrases, we need more augmentation or a bigger model.

2. **Dense 8B vs MoE 30B-A3B at same active params?** Run both and compare. If the 8B dense matches the 30B MoE, use the dense — it's 4x smaller on disk.

3. **How much session data is usable?** Most Pi calls are `bash` with arbitrary commands. The mapping to cadmus ops needs careful filtering. We might get 2K clean examples instead of 5K.

4. **Should the fine-tuned model still get a system prompt?** Probably a minimal one listing op names only (no descriptions), as a reminder. But the model should be able to pick the right tool without detailed descriptions.

5. **LoRA vs full fine-tune for this task size?** 8K examples is small. Full fine-tune on 0.6B might work better than LoRA on 8B. The validation run tests this.
