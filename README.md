# Cadmus

A pure-Rust reasoning engine that plans typed operations using unification, algebraic properties, and strategy-pattern domain specialization. Cadmus takes natural language commands or s-expression plan definitions and compiles them into executable Racket programs through a type-directed pipeline. Optionally, a local LLM (Qwen 3B, 2GB) acts as a fuzzy NL frontend for free-form input.

## Executive Summary

Cadmus exists to bridge the gap between human intent and executable programs. Given a command like *"find all PDFs in ~/Documents"* or a YAML plan definition, it:

1. **Parses** the input through a deterministic NL pipeline (Earley parser + fallback) or YAML loader
2. **Compiles** a type-checked operation pipeline using unification over a compositional type grammar
3. **Generates** a Racket program that performs the requested operations
4. **Executes** the program (optionally) via the Racket runtime

The engine is **data-driven**: operations, type signatures, domain knowledge, and NL vocabulary are defined in YAML files. Adding a new operation or file type requires zero Rust recompilation.

**Current state**: Active development (v0.7.0, 81 commits). Three working strategies (comparison, coding, filesystem), a natural language chat interface, 130+ YAML plan definitions (including 108 algorithm templates), and Racket code generation with execution. ~1450 tests passing, 1 known flaky test. No CI/CD pipeline.

**Who it's for**: Developers exploring type-directed program synthesis, domain-specific reasoning engines, and deterministic NL-to-code pipelines.

## Quick Start

### Prerequisites

- **Rust** (edition 2021) — for building Cadmus
- **Racket** (optional) — for executing generated programs (`brew install racket` on macOS)

### Build

```bash
cargo build --release                  # standard build
cargo build --release --features llm   # with local LLM frontend (requires ~/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf)
```

### Install (optional)

```bash
./scripts/install.sh    # Installs to ~/.local/bin/cadmus
```

### Run

**Interactive chat mode** (default):
```bash
cargo run
# or after install:
cadmus
```

**With local LLM** — understands free-form input like "yo search my code for async":
```bash
cargo run --features llm
```

**Plan mode** — compile and run an s-expression plan:
```bash
cargo run -- --plan data/plans/grep_code.sexp            # compile + execute
cargo run -- --plan data/plans/grep_code.sexp --dry-run   # compile only, show script
```

**Demo mode** — run all three strategy demos:
```bash
cargo run -- --demo
```

### Verify

```bash
cargo test
# Expected: ~1452 passed, 1 failed (known flaky: test_type_symmetric_discovery_tabular)
```

## Development Workflow

| Command | Purpose |
|---------|---------|
| `cargo build` | Build debug |
| `cargo build --release` | Build release |
| `cargo build --features llm` | Build with local LLM frontend |
| `cargo test` | Run all tests (~1452) |
| `cargo test -- --test-threads=1` | Run tests sequentially (for debugging) |
| `cargo test <name>` | Run specific test |
| `cargo test --test nl_tests` | Run NL integration tests |
| `cargo test --test stress_pipeline` | Run stress tests |
| `cargo clippy` | Lint |
| `cargo fmt` | Format |

## Architecture Overview

Cadmus has five major subsystems connected through a typed pipeline:

```
                    ┌──────────────────────────────────────────┐
                    │              User Input                   │
                    │  NL ("find PDFs in ~/docs")  or  YAML    │
                    └──────────┬──────────────┬────────────────┘
                               │              │
                    ┌──────────▼──────┐  ┌────▼──────────────┐
                    │   NL Pipeline   │  │   YAML Loader     │
                    │  (src/nl/)      │  │  (src/plan.rs)    │
                    └──────────┬──────┘  └────┬──────────────┘
                               │              │
                               ▼              ▼
                    ┌──────────────────────────────────────────┐
                    │              PlanDef (YAML)               │
                    │  name, inputs, steps, bindings            │
                    └──────────────────┬───────────────────────┘
                                       │
                    ┌──────────────────▼───────────────────────┐
                    │          Plan Compiler                    │
                    │  Type-check via unification               │
                    │  Resolve ops, infer types, promote Bytes  │
                    │  (src/plan.rs :: compile_plan)             │
                    └──────────────────┬───────────────────────┘
                                       │
                    ┌──────────────────▼───────────────────────┐
                    │          CompiledPlan                     │
                    │  Typed step chain with input/output types │
                    └──────────────────┬───────────────────────┘
                                       │
                    ┌──────────────────▼───────────────────────┐
                    │          Racket Executor                  │
                    │  CompiledPlan → Racket s-expressions      │
                    │  Shell bridges + native forms              │
                    │  (src/racket_executor.rs)                  │
                    └──────────────────┬───────────────────────┘
                                       │
                    ┌──────────────────▼───────────────────────┐
                    │          CallingFrame                     │
                    │  Bind inputs, write temp file, exec racket│
                    │  (src/calling_frame.rs)                    │
                    └──────────────────────────────────────────┘
```

### Core Subsystems

| Subsystem | Key Files | Purpose |
|-----------|-----------|---------|
| **Type System** | `src/type_expr.rs` | Compositional type grammar (`Primitive`, `Constructor`, `Var`) with unification |
| **Operation Registry** | `src/registry.rs` | Central registry of typed operations, loaded from YAML ops packs |
| **Plan Compiler** | `src/plan.rs` | YAML plan → type-checked pipeline with Bytes promotion |
| **Racket Executor** | `src/racket_executor.rs` | CompiledPlan → executable Racket script |
| **NL Pipeline** | `src/nl/` (13 modules) | Natural language → PlanDef via Earley parser + fallback |
| **Type Lowering** | `src/type_lowering.rs` | Maps rich fs_ops types to flat shell-callable types |
| **Strategies** | `src/strategy.rs`, `src/coding_strategy.rs`, `src/fs_strategy.rs` | Domain-specific reasoning (comparison, coding, filesystem) |
| **Theory** | `src/theory.rs` | Cross-entity reasoning from fact pack data |
| **Algebra** | `src/algebra.rs` | Plan canonicalization and rewrite rules |
| **Inference** | `src/racket_strategy.rs` | Op discovery and type-symmetric inference from fact packs |

### Type System

The type grammar is **open** — new types are just strings, no enum variants to add:

```
TypeExpr = Primitive(String)                    -- Bytes, Path, Name, Text, Number, ...
         | Constructor(String, Vec<TypeExpr>)   -- File(a), Seq(Entry(Name, a)), ...
         | Var(String)                          -- a, b, fmt (bound during unification)
```

Unification with occurs check drives type inference throughout the pipeline.

### Strategies

Three domain strategies implement the `ReasonerStrategy` trait:

- **ComparisonStrategy** (`src/strategy.rs`) — Compares entities across axes using fact pack data. Uses the monomorphic planner.
- **CodingStrategy** (`src/coding_strategy.rs`) — Analyzes source code for smells and refactoring. Uses the monomorphic planner.
- **FilesystemStrategy** (`src/fs_strategy.rs`) — Plans filesystem operations. Uses the polymorphic planner with type unification.

### Inference Engine

The Racket strategy (`src/racket_strategy.rs`) implements a multi-phase inference pipeline:

1. **Phase 0 — Discovery**: Scan fact pack for entities with `op_name` + `racket_symbol` not in registry
2. **Phase 1 — Op-symmetric**: Derive ops from symmetric partners (e.g., subtract from add)
3. **Phase 2 — Type-symmetric**: Derive ops from same type-symmetry class (e.g., multiply from add)
4. **Phase 3 — Op-symmetric replay**: Catch ops whose partners were discovered in Phase 2
5. **Phase 4 — Shell submodes**: Discover CLI tool submodes from macOS CLI fact pack

## Code Editing Ops

16 operations distilled from ~200 real coding agent sessions (3,015 edits across 41 projects). These cover the core loop agents actually perform: search → navigate → edit → verify.

| Category | Op | What It Does |
|----------|-----|-------------|
| Search | `grep_code` | Recursive grep across source files |
| Search | `find_definition` | Find where a fn/struct/class is defined |
| Search | `find_usages` | Find all references to a symbol |
| Search | `find_imports` | Find import/use statements for a module |
| Navigate | `file_outline` | Show fn/type definitions with line numbers |
| Navigate | `list_source_files` | List source files, excluding build dirs |
| Navigate | `recently_changed` | Files changed in last 5 git commits |
| Edit | `sed_replace` | Find/replace pattern in a file |
| Edit | `fix_import` | Replace an import path |
| Edit | `add_after` | Insert a line after a pattern match |
| Edit | `remove_lines` | Delete lines matching a pattern |
| Edit | `fix_assertion` | Update test expected values |
| Build | `build_project` | Auto-detect build system and build |
| Build | `test_project` | Auto-detect and run tests |
| Build | `lint_project` | Auto-detect and run linter |

Build/test/lint auto-detect the project type: `Cargo.toml` → cargo, `package.json` → npm, `Makefile` → make, `go.mod` → go.

Try them:
```bash
cargo run -- --plan data/plans/grep_code.sexp           # search for TODO
cargo run -- --plan data/plans/find_definition.sexp     # find where "main" is defined
cargo run -- --plan data/plans/build_project.sexp       # auto-detect and build
cargo run -- --plan data/plans/file_outline.sexp        # show structure of a file
```

Defined in `data/packs/ops/code_editing.ops.yaml`. See [AGENT_DISTILLED.md](AGENT_DISTILLED.md) for the full methodology.

## Local LLM Frontend

Optional feature that adds a local Qwen2.5-3B model as a fuzzy NL frontend. When the deterministic Earley parser can't understand input, the LLM takes over.

```bash
cargo run --features llm
```

**How it works:**

```
You: "yo can you look through my codebase for anything using async"

[llm] loading model... ready.
[llm] → grep_code  dir=.  pattern=async

✓ Plan created
(define (grep_code (dir : String) (pattern : String))
  (bind dir ".")
  (bind pattern "async")
  (grep_code))
```

The LLM does NOT generate plans or code. It does two things:
- **Fuzzy op matching**: "look through my codebase" → `grep_code`
- **Slot extraction**: "anything using async" → `pattern: async`

Rust then mechanically builds the typed plan, validates it against the registry, and hands it to the existing pipeline for type-checking and Racket codegen.

**More examples:**
```
"where is compile_plan defined in src"     → find_definition(dir=src, name=compile_plan)
"replace old_api with new_api in main.rs"  → sed_replace(file=main.rs, find=old_api, replace=new_api)
"compile my project and check for errors"  → build_project + test_project (multi-step!)
"what functions are in plan.rs"            → file_outline(file=plan.rs)
```

**Requirements:** A GGUF model at `~/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf` (2GB). Override with `CADMUS_LLM_MODEL=/path/to/model.gguf`.

**Architecture:** `src/nl/llm.rs`, feature-gated behind `--features llm`. Uses `llama-cpp-2` with Metal acceleration. Model loaded once via OnceLock (~2s), subsequent queries are instant. Zero network calls.

## Codebase Tour

### Directory Structure

```
src/
  main.rs              CLI entry point (chat, plan, demo modes)
  lib.rs               Module declarations
  plan.rs              Plan YAML DSL: parse, compile, execute (2000 lines)
  racket_executor.rs   CompiledPlan → Racket script (1910 lines)
  racket_strategy.rs   Inference engine: op discovery + type symmetry (1438 lines)
  registry.rs          Operation registry + YAML ops loader (1431 lines)
  generic_planner.rs   Monomorphic + polymorphic planners (1277 lines)
  theory.rs            Cross-entity theory derivation (1220 lines)
  algebra.rs           Plan canonicalization + rewrite rules (1026 lines)
  type_expr.rs         Type grammar, parsing, unification (978 lines)
  strategy.rs          ReasonerStrategy trait + ComparisonStrategy (897 lines)
  coding_strategy.rs   CodingStrategy (779 lines)
  fact_pack.rs         Fact pack YAML schema + indexed loader (780 lines)
  ui.rs                ANSI terminal formatting (740 lines)
  type_lowering.rs     fs_ops → shell-callable type bridge (608 lines)
  filetypes.rs         File type dictionary loader (598 lines)
  calling_frame.rs     Plan execution orchestration (429 lines)
  fs_strategy.rs       FilesystemStrategy + dry-run trace (404 lines)
  fs_types.rs          Registry builders (298 lines)
  types.rs             Core domain types (331 lines)
  planner.rs           Legacy obligation-based planner (347 lines)
  shell_helpers.rs     Shell quoting utilities (163 lines)
  line_editor.rs       Readline wrapper (167 lines)
  pipeline.rs          Entry point → strategy delegation (135 lines)

  nl/                  Natural language UX layer (9435 lines total)
    mod.rs             Entry point: process_input() (791 lines)
    dialogue.rs        Multi-turn state, build_plan, apply_edit (1665 lines)
    intent.rs          Keyword/pattern intent recognition (1082 lines)
    slots.rs           Slot extraction (paths, ops, modifiers) (926 lines)
    earley.rs          Earley parser engine (788 lines)
    intent_ir.rs       Structured intent representation (671 lines)
    normalize.rs       Tokenization, synonyms, case folding (671 lines)
    intent_compiler.rs IntentIR → PlanDef compilation (646 lines)
    typo.rs            SymSpell typo correction (612 lines)
    lexicon.rs         YAML lexicon loader (569 lines)
    grammar.rs         Earley grammar builder (360 lines)
    vocab.rs           Vocabulary YAML loader (340 lines)
    phrase.rs          Multi-word phrase tokenizer (314 lines)
    llm.rs             Local LLM fuzzy frontend [feature: llm] (320 lines)

data/
  filetypes.yaml       File type dictionary (197 entries, 14 categories)
  nl/
    nl_vocab.yaml      NL synonyms, contractions, approvals, rejections
    nl_dictionary.yaml SymSpell frequency dictionary (~2473 words)
    nl_lexicon.yaml    Earley parser lexicon (verbs, nouns, orderings)
  packs/
    ops/               Operation type signatures (YAML)
      fs.ops.yaml        49 filesystem ops
      code_editing.ops.yaml  16 code search/edit/build ops
      power_tools.ops.yaml  64 dev tool ops
      racket.ops.yaml    47 Racket language ops
      comparison.ops.yaml  6 comparison reasoning ops
      coding.ops.yaml    6 code analysis ops
    facts/             Domain knowledge (YAML)
      putin_stalin.facts.yaml   Political comparison
      macos_fs.facts.yaml       macOS tool knowledge
      macos_cli.facts.yaml      CLI tool submodes
      racket.facts.yaml         Racket op relationships
  plans/               Plan YAML definitions
    *.yaml             24 general-purpose plans
    algorithms/        108 algorithm templates across 14 categories

tests/                 19 test files (~11,800 lines)
scripts/
  install.sh           Build + install to ~/.local/bin
  validate_algorithm_plans.py  Algorithm plan validation
```

### Start Reading Here

1. **`src/main.rs`** — CLI entry point, shows all three modes
2. **`src/plan.rs`** — The plan compiler, heart of the type-checking pipeline
3. **`src/type_expr.rs`** — The type grammar and unification algorithm
4. **`src/racket_executor.rs`** — How plans become Racket programs
5. **`src/nl/mod.rs`** — NL pipeline entry point and Earley integration
6. **`src/registry.rs`** — How operations are registered and looked up
7. **`src/racket_strategy.rs`** — The inference engine (op discovery)
8. **`data/packs/ops/fs.ops.yaml`** — Example ops pack (the data format)
9. **`src/calling_frame.rs`** — How plans are executed end-to-end
10. **`src/type_lowering.rs`** — How rich types map to shell commands

## Configuration Overview

Cadmus is configured entirely through YAML data files. No environment variables or config files are required for basic operation.

| File | Purpose | Details |
|------|---------|---------|
| `data/packs/ops/*.ops.yaml` | Operation type signatures | [Configuration Guide](docs/configuration.md#ops-packs) |
| `data/packs/facts/*.facts.yaml` | Domain knowledge | [Configuration Guide](docs/configuration.md#fact-packs) |
| `data/filetypes.yaml` | File type dictionary | [Configuration Guide](docs/configuration.md#file-type-dictionary) |
| `data/nl/nl_vocab.yaml` | NL synonyms and vocabulary | [Configuration Guide](docs/configuration.md#nl-vocabulary) |
| `data/nl/nl_dictionary.yaml` | Typo correction dictionary | [Configuration Guide](docs/configuration.md#nl-dictionary) |
| `data/nl/nl_lexicon.yaml` | Earley parser lexicon | [Configuration Guide](docs/configuration.md#nl-lexicon) |
| `data/plans/*.yaml` | Plan definitions | [Plan DSL Reference](docs/plan-dsl.md) |

| `CADMUS_LLM_MODEL` env var | LLM model path | Default: `~/.models/Qwen2.5-3B-Instruct-Q4_K_M.gguf` |

All YAML files are loaded at runtime. The engine embeds fallback copies via `include_str!()` so it works without the data directory present, but prefers on-disk files when available.

## Testing Overview

```bash
cargo test                    # Run all ~1452 tests
cargo test --test nl_tests    # NL integration tests (184 tests)
cargo test --test stress_pipeline  # Stress tests (92 tests)
```

### Test Distribution

| Test File | Tests | Coverage |
|-----------|-------|----------|
| Unit tests (inline) | ~808 | All modules |
| `tests/nl_tests.rs` | 184 | NL pipeline end-to-end |
| `tests/stress_pipeline.rs` | 92 | All 7 pipeline subsystems |
| `tests/nl_earley_tests.rs` | 72 | Earley parser + phrase tokenizer |
| `tests/semantic_tests.rs` | 45 | Goal → plan → execution traces |
| `tests/shell_callable_tests.rs` | 41 | Shell-callable Racket forms |
| `tests/racket_tests.rs` | 35 | Racket ops, inference, scripts |
| `tests/fs_integration.rs` | 27 | Filesystem strategy |
| `tests/archive_codegen_tests.rs` | 24 | Archive format codegen |
| `tests/plan_tests.rs` | 19 | Plan compiler |
| `tests/power_tools_tests.rs` | 11 | Power tools ops |
| `tests/complex_programs.rs` | 16 | Multi-domain Racket programs |
| `tests/generic_planner_tests.rs` | 14 | Generic planner |
| `tests/compact_facts_tests.rs` | 15 | Compact fact pack format |
| `tests/integration.rs` | 5 | Original comparison strategy |
| Others | ~44 | Pipeline traces, reproductions |

**Known flaky test**: `test_type_symmetric_discovery_tabular` in `tests/shell_callable_tests.rs` — fails non-deterministically due to HashMap iteration order affecting inference path selection.

## Documentation Map

| Document | Description |
|----------|-------------|
| [Architecture](docs/architecture.md) | Detailed architecture: type system, registry, planners, strategies, inference engine, NL pipeline |
| [Configuration Guide](docs/configuration.md) | YAML data file schemas, formats, and extension recipes |
| [Plan DSL Reference](docs/plan-dsl.md) | Plan YAML syntax, compilation pipeline, type inference, and execution |
| [NL Pipeline](docs/nl-pipeline.md) | Natural language processing: Earley parser, intent recognition, dialogue state |
| [Playbook](PLAYBOOK.md) | Step-by-step recipes for adding ops, fact packs, plans, and file types |
| [Agent Distilled](AGENT_DISTILLED.md) | Code editing ops distilled from agent sessions: methodology, data, LLM frontend |
| [Bugs](BUGS.md) | Known NL layer bugs (16 tracked, 14 fixed) |
| [Semantics](SEMANTICS.md) | Semantic correctness analysis across all strategies |
| [Subsumption](SUBSUMPTION.md) | Shell-callable Racket forms migration plan |
| [Roadmap](ROADMAP.md) | Filesystem reasoner phase roadmap |

## Known Limitations

- **No CI/CD** — tests must be run locally
- **1 flaky test** — `test_type_symmetric_discovery_tabular` depends on HashMap iteration order (`tests/shell_callable_tests.rs:209`)
- **Algorithm plans partially working** — 44/108 compile, 41/108 execute successfully. Blocked by `infer_input_type()` not recognizing all input names (see `SEMANTICS.md`)
- **Racket required for execution** — generated scripts need `racket` installed; without it, only dry-run/codegen works
- **Single-developer project** — no LICENSE, CONTRIBUTING, or code review process
- **NL layer limitations** — see `BUGS.md` for 16 tracked issues (14 fixed, 1 deferred, 1 by-design)
- **Coding strategy output loss** — `execute_plan()` in `src/strategy.rs` only returns root node result; intermediate CodeSmell/Refactoring/TypeSignature lost in `assemble()` (documented in `SEMANTICS.md`)
- **LLM Metal cleanup assert** — When using `--features llm`, llama.cpp may print a harmless assert on process exit (Metal residency set cleanup). Does not affect plan creation or execution
