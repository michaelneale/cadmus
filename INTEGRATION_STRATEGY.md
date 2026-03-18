# Cadmus + Local LLM Agent

## What This Is

An agent loop where a small local LLM gives instructions and Cadmus does the heavy lifting. Three execution paths, chosen automatically:

1. **NL shortcut** — Cadmus's deterministic NL pipeline recognizes the task (e.g., "find PDFs in downloads") and executes instantly. Zero LLM cost, ~0.4s.
2. **LLM agent loop** — for tasks the NL pipeline can't parse, the LLM reasons step by step using `ACTION:` protocol. 3-30s depending on steps.
3. **Plan templates** — LLM can invoke pre-built multi-step plans (286 total) as single tools. One LLM turn → multiple typed ops.

```
User: "find comics in downloads"
  → NL pipeline recognizes it → execute directly (0.4s, no LLM)

User: "find where compile_plan is defined and show outline"
  → NL pipeline fails → LLM agent loop
  → step 1: find_definition → step 2: file_outline → done (8s)

User: "compute the mean of [1,2,3,4,5]"
  → NL pipeline fails → LLM agent loop
  → LLM sees stats ops injected in context → uses mean_list
```

## Quick Start

```bash
# Build
cargo build --release --features agent

# Ollama must be running (any OpenAI-compatible endpoint works)
ollama serve &

# Run
cadmus --agent "find where compile_plan is defined" --read-only
cadmus --agent "write hello world html to /tmp/hello.html and open it"
cadmus --agent "search for TODO in this project" --read-only
cadmus --tools                    # list base tools
cadmus --tools --read-only        # list read-only tools
```

### Environment

| Variable | Default | Description |
|---|---|---|
| `CADMUS_LLM_URL` | `http://localhost:11434/v1/chat/completions` | Any OpenAI-compatible endpoint |
| `CADMUS_MODEL` | `glm-4.7-flash:latest` | Model name |

## Architecture

### Execution Path Selection

```
cadmus --agent "task"
  │
  ├─ NL pipeline: process_input(task)
  │   ├─ PlanCreated? → execute directly → done (0.4s)
  │   └─ NeedsClarification? → fall through
  │
  └─ LLM agent loop (if NL failed)
      ├─ Build system prompt with context-aware tools
      │   ├─ Base 19 tools (always)
      │   ├─ Domain ops (if keywords match: git, stats, text, macos, web)
      │   └─ Plan templates (if plan names match task keywords)
      ├─ LLM outputs: ACTION: tool_name(param="value")
      ├─ Cadmus executes → annotates errors → returns RESULT
      ├─ LLM sees result → next ACTION or final answer
      └─ Repeat (max 15 steps)
```

### Tool Types

| Kind | Count | How they execute |
|---|---|---|
| **Registry ops** | 16 base + contextual | Full Cadmus pipeline: validate → PlanDef → compile → Racket → execute |
| **Synthetic ops** | 3 (write_file, read_file, shell) | Direct Rust execution, bypass pipeline |
| **Plan templates** | 286 (64 utility + 222 algorithm) | Load .sexp → bind params → compile → execute |

### Context-Aware Tool Selection

The system prompt is customized per task. Keywords in the task trigger domain-specific ops:

| Keywords | Injected Domain | Extra Ops |
|---|---|---|
| git, commit, push, merge... | power_tools (git) | git_init, git_add, git_commit... |
| mean, variance, percentile... | statistics | mean_list, median_list, variance_list... |
| csv, string, split, trim... | text_processing | string_split, csv_parse_row, word_count... |
| trash, spotlight, desktop... | macos_tasks | trash, find_recent, organize_by_extension... |
| http, server, route, port... | web | http_server, add_route... |

Plans whose names match task keywords are also surfaced as callable `plan:name(params)` tools.

### Error Annotation

When tools fail, Cadmus annotates errors with LLM-friendly hints:

| Error Pattern | Hint |
|---|---|
| No such file or directory | Check path, use read_file to verify |
| Missing required parameter | Check tool signature |
| Compilation error | Type mismatch, try simpler approach |
| Rust error[E...] | Read first error, use grep + sed to fix |
| Permission denied | File may be read-only |
| write op + read-only | Use read-only tools instead |

## Files

| File | Lines | Purpose |
|---|---|---|
| `src/tools.rs` | ~500 | Tool catalog, domain hints, contextual selection |
| `src/tool_executor.rs` | ~550 | Execution bridge, plan templates, error annotation |
| `src/agent.rs` | ~590 | NL shortcut, LLM agent loop, ACTION parser |
| `tests/agent_tools_tests.rs` | ~360 | 19 integration tests |

## Tests

```bash
cargo test --features agent --lib           # 839 tests (includes agent + tools)
cargo test --test agent_tools_tests          # 19 integration tests
# Total: 858 new+existing tests, 0 regressions
```

## Commits

1. `checkpoint` — agent mode working with glm-4.7-flash
2. `phase 1` — retire `--features llm` (remove llama-cpp-2 dep, 783 lines deleted)
3. `phase 2` — NL-first routing (try deterministic pipeline before LLM)
4. `phase 3+4` — context-aware tools + plans as composite tools
5. `phase 5` — error annotation (translate cryptic errors for LLM)
