# MoE Fine-Tuning Plan for Cadmus

> **Goal**: Train a small Mixture-of-Experts model that knows Cadmus's domain structure — routing to specialized experts for filesystem, code editing, devops, algorithms, git, etc. — so `cadmus --agent` gets a purpose-built tool-calling brain instead of a generic chat model.

---

## 1. Why MoE, Why Now

The `cadmus --agent` mode currently uses whatever Ollama model you point it at (GLM-4.7-Flash by default). That model knows nothing about Cadmus ops. The system prompt dumps a tool catalog, but the model guesses at tool selection and parameter formatting.

Cadmus has **8 clearly separated domains** with 600 ops, 345 plans, and a type system that constrains valid tool sequences. A dense model wastes capacity representing all domains simultaneously. MoE fits naturally: each domain can occupy different expert subspaces, and the router learns to activate the right experts based on the user's request.

**What we have to train on**:
- 94K real tool calls (44K from Pi agent sessions, 50K from Goose sessions)
- 600 op definitions with type signatures, descriptions, and parameter names
- 345 plans with NL descriptions paired to op sequences
- 1,273 verb synonyms and 31 concept→op dispatch rules in the NL lexicon

**Hardware**: Apple M4 Max, 64GB unified memory, 40 GPU cores, Metal 4.

---

## 2. What's On Disk

### Models (GGUF, `~/.models/`)

| Model | Size | Params | Active | Architecture |
|-------|------|--------|--------|-------------|
| Qwen3-30B-A3B | 17GB | 30B | 3B | **MoE**: 128 experts, top-8, 48 layers |
| Qwen3.5-27B | 16GB | 27B | 27B | Dense |
| Qwen2.5-32B | 19GB | 32B | 32B | Dense |
| Mistral-Small-3.1-24B | 14GB | 24B | 24B | Dense |
| Qwen3-8B | 4.7GB | 8B | 8B | Dense |
| OLMoE-1B-7B | 3.9GB | 7B | 1B | **MoE**: 64 experts, top-8 |
| Qwen2.5-Coder-7B | 4.3GB | 7B | 7B | Dense (code-tuned) |
| GLM-4.7-Flash | 4.5GB | 4.7B | 4.7B | Dense (current default) |
| Qwen2.5-3B | 1.9GB | 3B | 3B | Dense |
| Qwen3.5-0.8B | 0.5GB | 0.8B | 0.8B | Dense |
| Qwen3-0.6B | 0.4GB | 0.6B | 0.6B | Dense |
| Qwen2.5-0.5B | 0.3GB | 0.5B | 0.5B | Dense |

HuggingFace cache has Qwen3-30B-A3B metadata (config.json), but not the full safetensors weights (~60GB BF16). Those would need downloading for MLX-based training.

### Session Data

| Source | Sessions | Tool Calls | Tools | Format |
|--------|----------|-----------|-------|--------|
| Pi agent | 410 | 43,910 | 11 (bash, read, edit, write, mcp...) | JSONL per session |
| Goose | 7,855 | 49,759 | 108 (developer__shell, text_editor...) | SQLite DB (751MB) |
| **Total** | **8,265** | **93,669** | | |

### Tools Already Installed

| Tool | Status | What it does |
|------|--------|-------------|
| `llama-finetune` | ✓ Installed (brew llama.cpp) | GGUF-native full fine-tune. AdamW/SGD. Metal accelerated. Trains directly from quantized GGUF. No LoRA — full parameter update. |
| `llama-quantize` | ✓ Installed | Quantize models to Q4_K_M, Q8, etc. |
| `llama-export-lora` | ✓ Installed | Merge LoRA adapters back into base GGUF. |
| `llama-server` | ✓ Installed | Serve GGUF models via OpenAI-compatible API. |
| `ollama` | ✓ Installed | Model management + serving. |
| `uv` | ✓ Installed | Fast Python package manager (for setting up MLX etc.) |
| Python 3.11, 3.13 | ✓ Installed | Via homebrew. |
| MLX / mlx-lm | ✗ Not installed | `uv pip install mlx mlx-lm` — Apple's native ML framework. |
| HF transformers + PEFT | ✗ Not installed | `uv pip install transformers peft trl` — standard LoRA stack. |
| Unsloth | ✗ Not installed | Fast LoRA fine-tuning with MLX backend. |

---

## 3. Training Tool Options

### Option A: `llama-finetune` (lowest friction)

**Already installed.** Trains directly from the GGUF files in `~/.models/`. No need to download safetensors, no Python environment to set up, no HuggingFace login.

```bash
# Train from existing GGUF — works TODAY
llama-finetune \
    -m ~/.models/Qwen3-8B-Q4_K_M.gguf \
    -f data/training/cadmus_train.txt \
    -o cadmus-8b-finetuned.gguf \
    --epochs 2 \
    --learning-rate 1e-5 \
    --optimizer adamw \
    --batch-size 4
```

**Pros**: Zero setup. Trains on quantized weights (fits in memory easily). Metal accelerated. Output is a ready-to-use GGUF.

**Cons**: Full fine-tune only (no LoRA). Can only train on plain text continuation — no structured chat template support. Modifies all weights, so catastrophic forgetting risk is higher. No MoE-specific expert targeting. Limited to what llama.cpp supports (no DPO, no reward modeling).

**Data format**: Plain text file. We'd format conversations using the model's chat template tokens (`<|im_start|>`, `<|im_end|>` for Qwen).

**Best for**: Quick experiment on a small model (Qwen3-0.6B, Qwen3.5-0.8B) to validate the training data pipeline before investing in the full LoRA setup.

### Option B: MLX + mlx-lm (recommended primary)

Apple's native ML framework. Runs on Metal, uses unified memory (the whole 64GB is available to the GPU). Supports LoRA fine-tuning with configurable target layers.

```bash
# Setup (one-time)
uv venv ~/.venvs/cadmus-train --python 3.11
source ~/.venvs/cadmus-train/bin/activate
uv pip install mlx mlx-lm

# LoRA fine-tune
mlx_lm.lora \
    --model Qwen/Qwen3-30B-A3B \  # downloads safetensors from HF
    --data data/training/ \
    --adapter-path adapters/ \
    --lora-layers 16 \
    --batch-size 1 \
    --iters 1000 \
    --learning-rate 2e-5

# Merge adapters into base
mlx_lm.fuse \
    --model Qwen/Qwen3-30B-A3B \
    --adapter-path adapters/ \
    --output-path merged/

# Convert to GGUF for Ollama
mlx_lm.convert --hf-path merged/ --mlx-path mlx-model/ -q 4
# Then use llama-quantize or manual GGUF conversion
```

**Pros**: Native Apple Silicon. LoRA means only ~1-5% of weights are updated (less forgetting). Can target specific layers including MoE expert FFNs and router. JSONL chat format input. Supports Qwen3-MoE architecture. The 30B-A3B model would need ~17GB for 4-bit base + ~2GB for adapters + activations — fits in 64GB.

**Cons**: Needs to download full safetensors (~60GB for Qwen3-30B-A3B). Python environment setup. MLX LoRA's MoE expert targeting may need manual config. Conversion back to GGUF adds a step.

**Data format**: JSONL with `{"messages": [{"role": "system", ...}, {"role": "user", ...}, {"role": "assistant", ...}]}` or plain `{"text": "..."}`.

**Best for**: The actual production fine-tune. LoRA on Qwen3-30B-A3B gives us the MoE routing benefit with minimal forgetting.

### Option C: HuggingFace transformers + PEFT + TRL

The most flexible option. Full control over which layers get LoRA, DPO training for preference optimization, and the most documentation/community support.

```bash
# Setup
uv venv ~/.venvs/cadmus-train --python 3.11
source ~/.venvs/cadmus-train/bin/activate
uv pip install torch transformers peft trl datasets accelerate bitsandbytes

# Config (train.py or YAML)
from peft import LoraConfig, get_peft_model
lora_config = LoraConfig(
    r=32,
    lora_alpha=64,
    target_modules=[
        "q_proj", "k_proj", "v_proj", "o_proj",  # attention
        "gate",                                    # MoE router
        "up_proj", "down_proj",                    # expert FFN
    ],
    lora_dropout=0.05,
    task_type="CAUSAL_LM",
)
```

**Pros**: Can target MoE router (`gate`) and expert FFN layers explicitly. DPO support for preference training. The best-documented approach. Can do SFT + DPO in sequence. Works with any model architecture.

**Cons**: PyTorch MPS backend is slower than MLX on Apple Silicon. Memory management is worse (PyTorch doesn't use unified memory as efficiently). Larger dependency tree. May need `bitsandbytes` workarounds for Apple Silicon (limited MPS support for quantized training).

**Data format**: HF Datasets or JSONL. ShareGPT format for multi-turn conversations. Separate format for DPO (chosen/rejected pairs).

**Best for**: If we need DPO training (wrong-tool rejection learning) or very precise control over MoE expert layer targeting. Fallback if MLX can't target expert layers properly.

### Option D: Unsloth

Optimized LoRA training. Claims 2x speed. Has an MLX backend for Apple Silicon.

**Pros**: Fastest LoRA training. Good defaults.

**Cons**: Newer, less documented for MoE. Apple Silicon support is secondary. Another dependency.

**Best for**: If MLX-LM is too slow and we want to iterate faster.

### Recommendation

**Start with Option A** (`llama-finetune`) on Qwen3-0.6B to validate the data pipeline in 30 minutes. Then **move to Option B** (MLX + mlx-lm) for the real LoRA fine-tune on Qwen3-30B-A3B. Fall back to **Option C** (HF + PEFT) only if MLX can't target MoE expert layers.

---

## 4. Domain → Expert Mapping

Qwen3-30B-A3B has 128 experts per MoE layer, top-8 routing. We don't directly assign experts to domains — the router learns this from training data. But we structure data so each domain has distinct patterns:

```
Cadmus Domains          Expert Subspaces (learned)
─────────────          ─────────────────────────
filesystem (87 ops)    ──→  Experts that know file paths, glob patterns, walk_tree
code_editing (21 ops)  ──→  Experts that know grep, AST, symbol names
devops (52 ops)        ──→  Experts that know docker, gh, ssh, deploy
algorithms (207 ops)   ──→  Experts that know data structures, math
git (64 ops)           ──→  Experts that know commits, branches, diffs
racket (145 ops)       ──→  Experts that know list ops, higher-order functions
text (15 ops)          ──→  Experts that know string manipulation
reasoning (6 ops)      ──→  Experts that know comparison, evidence
```

The hypothesis: after fine-tuning, different experts will activate for different domains. We can validate this by extracting router logits per domain and checking entropy.

---

## 5. Training Data Generation

### 5.1 Source A: Op Registry → Single-Turn Tool Calls

For each of the 600 ops, generate 3-5 paraphrased instruction→ACTION pairs using the NL lexicon's verb synonyms.

**Example** (from `docker_ps` op + lexicon synonyms for "enumerate"):

```jsonl
{"messages":[{"role":"user","content":"List running docker containers"},{"role":"assistant","content":"ACTION: docker_ps()"}]}
{"messages":[{"role":"user","content":"Show me the docker containers"},{"role":"assistant","content":"ACTION: docker_ps()"}]}
{"messages":[{"role":"user","content":"Display all containers with status"},{"role":"assistant","content":"ACTION: docker_ps()"}]}
```

Estimated: 600 ops × 4 variants = **~2,400 examples**.

### 5.2 Source B: Plan Files → Multi-Step Traces

For each of the 345 plans, extract the NL description and op sequence, then format as a multi-turn conversation with synthetic RESULT outputs.

**Example** (from `review_pr.sexp`):

```jsonl
{"messages":[
  {"role":"user","content":"Review pull request number 42"},
  {"role":"assistant","content":"I'll review this PR step by step.\nACTION: gh_pr_view(dir=\".\", pr_number=\"42\")"},
  {"role":"user","content":"RESULT: PR #42: Fix memory leak in cache module..."},
  {"role":"assistant","content":"Now let me check the diff.\nACTION: gh_pr_diff(dir=\".\", pr_number=\"42\")"},
  {"role":"user","content":"RESULT: +++ b/src/cache.rs\n@@ -45,7 +45,8 @@..."},
  {"role":"assistant","content":"And the CI status.\nACTION: gh_pr_checks(dir=\".\", pr_number=\"42\")"},
  {"role":"user","content":"RESULT: All 12 checks passed"},
  {"role":"assistant","content":"PR #42 looks good. The fix properly drops the Arc reference before the cache entry is evicted."}
]}
```

Estimated: 345 plans → **~345 multi-turn examples** (plus paraphrased variants: ~800).

### 5.3 Source C: Session Traces → Real User Workflows

Extract (user_message, tool_sequence) pairs from Pi and Goose sessions. Map tool names to Cadmus ops where possible:

| Session Tool | Maps To |
|-------------|---------|
| `bash(command="grep -r ...")` | `grep_code(dir=..., pattern=...)` |
| `bash(command="docker ps")` | `docker_ps()` |
| `bash(command="git log ...")` | `git_log(dir=...)` |
| `read(path=...)` | `read_file(file=...)` |
| `edit(path=..., oldText=..., newText=...)` | `sed_replace(file=..., find=..., replace=...)` |
| `developer__shell(command=...)` | mapped by command content |
| `developer__text_editor(command="view")` | `read_file(file=...)` |

**Filtering**: Drop sessions <3 tool calls. Drop tool calls that don't map to Cadmus ops. Keep the user message that preceded each tool sequence.

Estimated: ~94K tool calls, ~5-10% mappable to Cadmus ops = **~5,000-9,000 examples** after filtering and deduplication.

### 5.4 Source D: Type Constraint DPO Pairs

For each domain, generate wrong-tool examples as rejected responses:

```jsonl
{"prompt":"List docker containers","chosen":"ACTION: docker_ps()","rejected":"ACTION: list_dir(path=\".\")"}
{"prompt":"Find TODOs in the code","chosen":"ACTION: find_todos(dir=\".\")","rejected":"ACTION: walk_tree(path=\".\")"}
```

Estimated: **~300 DPO pairs** (10 per domain × 30 concepts).

### 5.5 Total Estimated Training Data

| Source | Examples |
|--------|---------|
| Op registry (augmented) | 2,400 |
| Plan files (augmented) | 800 |
| Session traces (filtered) | 5,000 |
| DPO pairs | 300 |
| **Total SFT** | **~8,200** |
| **Total DPO** | **~300** |

---

## 6. Training Pipeline

### Phase 1: Data Generation (Day 1)

```bash
# Generate all training data
python3 scripts/gen_training_data.py --source ops    --output data/training/ops.jsonl
python3 scripts/gen_training_data.py --source plans   --output data/training/plans.jsonl
python3 scripts/gen_training_data.py --source sessions --output data/training/sessions.jsonl
python3 scripts/gen_training_data.py --source dpo     --output data/training/dpo.jsonl

# Merge, shuffle, split 90/10
python3 scripts/merge_training_data.py \
    --inputs data/training/*.jsonl \
    --output data/training/train.jsonl data/training/valid.jsonl \
    --val-split 0.1
```

### Phase 2: Quick Validation on Small Model (Day 1-2)

Use `llama-finetune` on Qwen3-0.6B (400MB GGUF) to validate data quality:

```bash
# Format data as plain text with Qwen chat template
python3 scripts/format_for_llama_finetune.py \
    --input data/training/train.jsonl \
    --output data/training/train_qwen.txt \
    --template qwen3

# Fine-tune (~30 min on M4 Max)
llama-finetune \
    -m ~/.models/Qwen3-0.6B-Q4_K_M.gguf \
    -f data/training/train_qwen.txt \
    -o ~/.models/cadmus-0.6b-v1.gguf \
    --epochs 2 \
    --learning-rate 1e-5 \
    --batch-size 8

# Quick eval
cadmus --agent --model ~/.models/cadmus-0.6b-v1.gguf --eval
```

### Phase 3: MLX LoRA on Target Model (Day 2-4)

```bash
# Setup environment
uv venv ~/.venvs/cadmus-train --python 3.11
source ~/.venvs/cadmus-train/bin/activate
uv pip install mlx mlx-lm

# Download Qwen3-30B-A3B safetensors (~60GB, one-time)
# mlx_lm.lora will auto-download from HuggingFace

# LoRA fine-tune (~4-8 hours on M4 Max 64GB)
mlx_lm.lora \
    --model Qwen/Qwen3-30B-A3B \
    --train \
    --data data/training/ \
    --adapter-path adapters/cadmus-v1/ \
    --lora-layers 16 \
    --batch-size 1 \
    --iters 2000 \
    --learning-rate 2e-5 \
    --val-batches 25 \
    --steps-per-report 10 \
    --steps-per-eval 100 \
    --save-every 500

# Test before merging (runs with adapter on-the-fly)
mlx_lm.generate \
    --model Qwen/Qwen3-30B-A3B \
    --adapter-path adapters/cadmus-v1/ \
    --prompt "List all running docker containers"
```

### Phase 4: Merge and Deploy (Day 4-5)

```bash
# Merge LoRA adapters into base model
mlx_lm.fuse \
    --model Qwen/Qwen3-30B-A3B \
    --adapter-path adapters/cadmus-v1/ \
    --output-path merged/cadmus-30b-a3b-v1/

# Convert to GGUF (for Ollama/llama.cpp)
# Option 1: MLX native export
mlx_lm.convert \
    --hf-path merged/cadmus-30b-a3b-v1/ \
    --quantize q4_k_m \
    --output merged/cadmus-30b-a3b-v1-q4.gguf

# Option 2: Use llama.cpp convert
python3 /opt/homebrew/share/llama.cpp/convert_hf_to_gguf.py \
    merged/cadmus-30b-a3b-v1/ \
    --outfile ~/.models/cadmus-30b-a3b-v1.gguf \
    --outtype q4_k_m

# Create Ollama model
cat > Modelfile << 'EOF'
FROM ~/.models/cadmus-30b-a3b-v1.gguf
PARAMETER temperature 0.1
PARAMETER num_ctx 4096
SYSTEM "You complete tasks step by step using Cadmus tools."
EOF
ollama create cadmus:v1 -f Modelfile

# Update cadmus config
export CADMUS_MODEL=cadmus:v1
cadmus --agent "find all TODOs in the codebase"
```

### Phase 5: Evaluate (Day 5)

```bash
# Baseline (untuned)
CADMUS_MODEL=qwen3-30b-a3b cadmus --agent --eval 2>&1 | tee eval_baseline.txt

# Fine-tuned
CADMUS_MODEL=cadmus:v1 cadmus --agent --eval 2>&1 | tee eval_finetuned.txt

# Compare
diff eval_baseline.txt eval_finetuned.txt
```

---

## 7. Memory Budget

| Model | Method | Base Weights | LoRA Adapters | Activations | Total | Fits 64GB? |
|-------|--------|-------------|---------------|-------------|-------|-----------|
| Qwen3-0.6B | Full FT (GGUF) | 0.4GB | — | ~1GB | ~5GB | ✓ easily |
| Qwen3.5-0.8B | Full FT (GGUF) | 0.5GB | — | ~1GB | ~6GB | ✓ easily |
| Qwen3-8B | LoRA (MLX) | 5GB (4-bit) | 0.1GB | ~4GB | ~9GB | ✓ |
| OLMoE-1B-7B | LoRA (MLX) | 4GB (4-bit) | 0.1GB | ~3GB | ~7GB | ✓ |
| Qwen3-30B-A3B | LoRA (MLX) | 17GB (4-bit) | 0.3GB | ~8GB | ~25GB | ✓ |
| Qwen3-30B-A3B | LoRA (BF16) | 60GB | — | — | >64GB | ✗ (need 4-bit) |

MLX uses unified memory — the 64GB is shared between CPU and GPU. With 4-bit quantized base weights, even the 30B MoE model fits comfortably with room for batch activations.

---

## 8. MoE-Specific Considerations

### Router Training

Qwen3-30B-A3B's config already includes `router_aux_loss_coef: 0.001` — a load-balancing loss that prevents expert collapse. During LoRA fine-tuning, if we target the `gate` (router) weights:

- The router learns to associate Cadmus domain patterns with specific expert subspaces
- The aux loss ensures experts don't collapse (some experts get all tokens while others idle)
- We should verify that MLX-LM's LoRA allows targeting `gate` modules — if not, fall back to HF PEFT

### Expert-Targeted LoRA

Standard LoRA targets attention projections. For MoE, we also want to adapt the expert FFN layers:

```
Default LoRA targets:  q_proj, k_proj, v_proj, o_proj
MoE LoRA targets:      + gate, up_proj, down_proj (per-expert)
```

This is where the tool choice matters:
- **MLX-LM**: configurable `--lora-modules` flag (need to verify MoE expert layer names)
- **HF PEFT**: explicit `target_modules` list in `LoraConfig`
- **llama-finetune**: full fine-tune only (all weights updated, including experts)

### Domain-Aware Batching

Group training examples by domain within each batch so the router gets clear signal:

```
Batch 1: [docker_ps example, docker_stop example, docker_build example, docker_logs example]
Batch 2: [gh_pr_list example, gh_issue_create example, gh_run_view example, gh_release_list example]
Batch 3: [walk_tree example, find_matching example, sort_by example, list_dir example]
```

This can be implemented in the data generation script by tagging examples with domains and using a custom sampler.

---

## 9. Success Criteria

| Metric | Baseline (untuned) | Target | How to measure |
|--------|-------------------|--------|---------------|
| Single-op tool selection | ~60% | >90% | 600 ops × 1 prompt each |
| Parameter accuracy | ~50% | >85% | Correct param names + types |
| Multi-step sequence | ~30% | >70% | 345 plans, compare op sequence |
| First-token latency | ~800ms | <1200ms | Time to first ACTION token |
| Expert specialization | ~uniform | Measurable clustering | Router logit entropy per domain |

---

## 10. Risks

| Risk | Mitigation |
|------|-----------|
| **Not enough data for MoE routing** | Start with dense Qwen3-8B LoRA. If dense matches MoE at 8K examples, we need more data. |
| **MLX can't target MoE expert layers** | Fall back to HF PEFT. Or use `llama-finetune` for full fine-tune on smaller model. |
| **Catastrophic forgetting** | LoRA (low rank) limits drift. Eval general ability alongside tool accuracy. |
| **GGUF quantization destroys LoRA signal** | Merge at full precision first, then quantize. Test Q8 before Q4. |
| **60GB download for safetensors** | Start with OLMoE-1B-7B (smaller) or Qwen3-8B (dense). Scale up once data pipeline works. |
| **Session data too noisy** | Heavy filtering: only keep tool calls that map to Cadmus ops. Require ≥3 tool calls per session. |
| **Training takes too long** | Reduce to 8-16 LoRA layers. Reduce iters. Use smaller batch. M4 Max should handle 30B-A3B QLoRA in 4-8 hours. |

---

## 11. Timeline

| Phase | Work | Duration |
|-------|------|----------|
| 0. Environment setup | `uv venv` + install MLX/mlx-lm | 30 min |
| 1. Data generation | Write `gen_training_data.py`, produce ~8K examples | 1 day |
| 2. Quick validation | `llama-finetune` on Qwen3-0.6B, verify data quality | 2 hours |
| 3. Dense baseline | MLX LoRA on Qwen3-8B | 1 day |
| 4. MoE fine-tune | MLX LoRA on Qwen3-30B-A3B | 2 days |
| 5. Eval + iterate | Compare models, adjust data/hyperparams | 2 days |
| 6. Deploy | Merge, quantize, Ollama model, update cadmus | half day |
| **Total** | | **~7 days** |

---

## 12. Open Questions

1. **Does `mlx_lm.lora` support targeting MoE `gate` and expert FFN layers?** Need to check source or test empirically. The `--lora-modules` flag may accept these.

2. **Should the model output ACTION protocol or sexpr plans?** ACTION is what `cadmus --agent` speaks today. But the model could also output `(define (task ...) (op1) (op2))` — the plan compiler would type-check it. More constrained = fewer errors, but harder to learn.

3. **Is full fine-tune via `llama-finetune` competitive with LoRA for this task?** The data is small and focused. Full fine-tune on a tiny model (0.6B) might outperform LoRA on a bigger model for this specific tool-routing task.

4. **How much of the 94K session data is actually usable?** Most Pi calls are `bash` with arbitrary commands. The mapping to Cadmus ops needs careful filtering. We might only get 5K clean examples from sessions.

5. **Should we train a router-only model?** Instead of fine-tuning the whole LLM, train a tiny classifier (Qwen3-0.6B or even a linear probe on embeddings) that just picks the right tool. The big model handles reasoning/parameters. This is the "Mixture of LoRA Experts" approach — each domain gets its own tiny LoRA adapter, and a learned router picks which adapter to activate per query.
