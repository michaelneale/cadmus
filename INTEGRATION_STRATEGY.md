# Cadmus + Local LLM Agent

## What This Is

An agent loop where a small local LLM gives instructions and Cadmus does the heavy lifting. The LLM says what it wants in plain text (`ACTION: write_file(path=..., content=...)`), Cadmus validates, compiles, executes, and returns the result. The LLM reads the result, decides the next step. Repeat until done.

**Key insight**: small models (3-4B) are bad at structured tool-calling JSON but good at outputting single-line text instructions. The `ACTION:` protocol is trivially parseable and LLM-friendly. Cadmus handles everything else — type checking, Racket codegen, shell quoting, file safety.

## Architecture

```
User: "write a pacman game and open it"
  │
  ▼
┌──────────────┐  ACTION: write_file(path=..., content=...)  ┌──────────────┐
│  Local LLM   │ ──────────────────────────────────────────▸ │   Cadmus     │
│  (glm-4.7    │                                              │   Engine     │
│   -flash)    │ ◂────────────────────────────────────────── │  (19 tools)  │
│              │  RESULT: Wrote 2847 bytes to /tmp/pacman.html│              │
└──────────────┘                                              └──────────────┘
  │                                                             │
  │  ACTION: shell(command="open /tmp/pacman.html")             │
  │ ──────────────────────────────────────────────────────────▸ │
  │                                                             │
  │  RESULT: (opened in browser)                                │
  │ ◂────────────────────────────────────────────────────────── │
  │
  ▼
"Done. Created pacman game and opened in browser."
```

## Quick Start

```bash
# Build
cargo build --release --features agent

# Start local model (Ollama)
ollama serve

# Run
cadmus --agent "find where compile_plan is defined" --read-only
cadmus --agent "write hello world html to /tmp/hello.html and open it"
cadmus --agent "search for TODO comments in this project" --read-only

# List tools
cadmus --tools
cadmus --tools --read-only
```

### Environment

| Variable | Default | Description |
|---|---|---|
| `CADMUS_LLM_URL` | `http://localhost:11434/v1/chat/completions` | Any OpenAI-compatible endpoint |
| `CADMUS_MODEL` | `glm-4.7-flash:latest` | Model name |

## How It Works

The LLM and Cadmus communicate via a simple text protocol:

**LLM outputs**: `ACTION: tool_name(param="value", param="value")`
**Cadmus returns**: `RESULT: <execution output>`

When the LLM has no more actions, it gives a plain text final answer.

### Two kinds of tools

1. **Registry ops** (16) — go through the full Cadmus typed pipeline: validate op exists → build PlanDef with correct types → compile → generate Racket → execute → capture output. These are `grep_code`, `find_definition`, `file_outline`, `build_project`, etc.

2. **Synthetic ops** (3) — handled directly for things the typed pipeline can't express:
   - `write_file(path, content)` — write arbitrary text to a file (HTML, scripts, configs)
   - `read_file(path)` — read a file's contents
   - `shell(command)` — run any shell command

This split is deliberate. The LLM generates content (HTML for a game, a shell script, a config file). Cadmus writes it to disk and opens it. The typed pipeline handles structured operations (grep, find, build). Neither tries to do the other's job.

## Available Tools (19)

| Tool | Params | Kind | Description |
|---|---|---|---|
| `grep_code` | dir, pattern | registry | Search source files for pattern |
| `find_definition` | dir, name | registry | Find function/struct definition |
| `find_usages` | dir, symbol | registry | Find all references to a symbol |
| `find_imports` | file, module | registry | Find import statements |
| `file_outline` | file | registry | Show functions with line numbers |
| `list_source_files` | dir | registry | List source files |
| `recently_changed` | dir | registry | Files changed in last 5 commits |
| `sed_replace` | file, find, replace | registry | Find/replace in file |
| `fix_import` | file, old_path, new_path | registry | Fix import path |
| `add_after` | file, after_pattern, new_line | registry | Insert line after match |
| `remove_lines` | file, pattern | registry | Delete matching lines |
| `fix_assertion` | file, old_value, new_value | registry | Fix test assertions |
| `build_project` | dir | registry | Auto-detect and build |
| `test_project` | dir | registry | Auto-detect and test |
| `lint_project` | dir | registry | Auto-detect and lint |
| `open_file` | path | registry | Open with default app |
| `write_file` | path, content | synthetic | Write text to file |
| `read_file` | path | synthetic | Read file contents |
| `shell` | command | synthetic | Run shell command |

## Files

| File | Lines | Purpose |
|---|---|---|
| `src/tools.rs` | 370 | Tool catalog generation (OpenAI JSON + text format) |
| `src/tool_executor.rs` | 380 | Execution bridge (registry ops + synthetic ops) |
| `src/agent.rs` | 490 | Agent loop + ACTION parser + LLM communication |
| `tests/agent_tools_tests.rs` | 360 | 19 integration tests |
| **Total** | **~1600** | |

## Tests

```bash
cargo test --features agent --lib           # 823 tests (includes 25 new)
cargo test --test agent_tools_tests          # 19 integration tests
```

All 842 tests pass. Zero regressions in existing test suites.
