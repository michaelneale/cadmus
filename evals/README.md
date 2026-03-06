# Cadmus Agent Evals

Comparative evaluation of **Cadmus**, **pi**, and **Goose** running the same tasks
with the same local LLM: `glm-4.7-flash:latest` via Ollama.

## Setup

### Prerequisites

```bash
# Ollama with glm-4.7-flash
ollama pull glm-4.7-flash:latest

# Cadmus (from repo root)
cargo build --release

# pi
npm install -g @mariozechner/pi-coding-agent

# Goose (optional — for comparison)
brew install goose
```

### Environment

- **Model**: `glm-4.7-flash:latest` (19GB, via Ollama)
- **Hardware**: Apple M4 Max, 64GB RAM
- **OS**: macOS
- **Ollama endpoint**: `http://localhost:11434`

## Running the Evals

```bash
# Run all evals
./evals/run_evals.sh

# Or run individual tasks:
./target/release/cadmus --agent "list files"
pi --provider ollama --model "glm-4.7-flash:latest" -p "list files"
GOOSE_PROVIDER=ollama GOOSE_MODEL=glm-4.7-flash:latest goose run -t "list files"
```

## Results — 2026-03-06

### Summary

| Tool    | Pass | Fail | Notes |
|---------|------|------|-------|
| Cadmus  | 8/10 | 2    | NL misfire on "show processes", Linux commands for wifi |
| pi      | 10/10| 0    | Reliable across all tasks |
| Goose   | 0/5  | 5    | glm-4.7-flash can't emit valid tool-call JSON for Goose |

### Detailed Results

#### 1. List Files
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | **0.3s** | [NL] — no LLM call, deterministic pipeline |
| pi | ✅ | ~3s | LLM |
| Goose | ❌ | 30s+ | "no shell tools available" — tool call parse failure |

**Winner: Cadmus** — NL shortcut fires instantly, 10x faster.

#### 2. Create File
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | 3.3s | [LLM] 1 tool call: `write_file` |
| pi | ✅ | ~4s | 1 tool call |
| Goose | ❌ | 60s+ | Repeated tool call parse errors, timeout |

**Tie: Cadmus ≈ pi.** Both handle it cleanly.

#### 3. Replace Text in File
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | 4.7s | [LLM] 3 calls: read → sed_replace → read (verified) |
| pi | ✅ | ~4s | edit tool |

**Tie.** Cadmus's read-verify-read pattern is slightly more cautious.

#### 4. Write Python Script + Run It
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | 8.1s | [LLM] wrote file + ran it |
| pi | ✅ | ~10s | wrote file + ran it |

**Tie.** Both wrote correct Fibonacci scripts and executed them.

#### 5. Create Folder + Move File
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | 5.5s | [LLM] 3 calls: mkdir+mv → ls → ls (verified) |
| pi | ✅ | ~5s | |

**Tie.**

#### 6. HTML + JS Project (Button Color Change)
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | 11.6s | Correct: `<script>` at end of `<body>`, works on load |
| pi | ✅ | 10.2s | Bug: `<script>` in `<head>` — button click fails (DOM not ready) |

**Winner: Cadmus** — pi's version has a subtle DOM-readiness bug.

#### 7. System Report (disk/memory/CPU)
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | 25.1s | Good report, NL misfired first (tried find_matching), LLM recovered. Report accurate but `$(date)` not expanded. |
| pi | ✅ | 25.4s | Much more detailed report: per-volume disk, VM stats, recommendations. Properly formatted date. |

**Winner: pi** — significantly more detailed report with proper formatting.
Cadmus had a literal `$(date)` in the output instead of expanding it, and the
report had less detail. Both got the core facts right.

#### 8. Top 5 CPU Processes
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ❌ | 0.3s | NL misfire: "show" matched to `list_dir`, returned file listing instead of processes |
| pi | ✅ | 23.4s | Correct: ran `ps` and formatted top 5 processes |

**Winner: pi.** Cadmus's NL pipeline incorrectly matched "show me the top 5
processes" to the `list_dir` op because "show" is a synonym for "list" in the
NL vocab. The NL shortcut returned a wrong answer with confidence (green ✓)
and never fell through to the LLM. This is worse than a failure — it's a
**false positive** from the NL pipeline.

#### 9. Hostname + IP
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | 6.6s | [LLM] Tried `hostname -I` (Linux), failed, recovered with `ifconfig` |
| pi | ✅ | 8.6s | |

**Tie.** Both got the right answer. Cadmus tried a Linux command first (the LLM
doesn't know it's on macOS) but self-corrected.

#### 10. Wifi Network Name
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ❌ | 30s+ | Tried 11 Linux commands (`iwgetid`, `nmcli`, `iwconfig`, `ip link`, `/proc/net/wireless`...) before timing out. Never tried the macOS command (`networksetup` or `system_profiler`). |
| pi | ✅ | 9.0s | Got answer quickly |

**Winner: pi.** Cadmus's LLM doesn't know it's on macOS and exhausted Linux
commands. The system prompt could include OS info to prevent this.

#### 11. Open Calculator App
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ❌ | 8.0s | Read local JS files looking for a "Calculator app" in the project. Never tried `open -a Calculator`. |
| pi | ✅ | 5.7s | Opened Calculator via `open -a Calculator` |

**Winner: pi.** Cadmus doesn't understand "open app" as a desktop automation
task. Its tool set is file/code focused — no `open` command awareness.

#### 12. Battery Status
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | 12.5s | Tried 4 Linux commands first, eventually found `pmset -g batt` on attempt 5 |
| pi | ✅ | 6.8s | Got it on first try |

**Winner: pi** on speed. Cadmus got the right answer but wasted 4 tries on
Linux commands first.

#### 13. Todo App (HTML + JS)
| Tool | Result | Time | Notes |
|------|--------|------|-------|
| Cadmus | ✅ | 13.9s | Clean minimal code, Enter key support |
| pi | ✅ | 26.7s | More features: delete button, XSS protection, auto-focus |

**Cadmus faster, pi more complete.** Both produced working apps. Pi added
delete functionality and XSS escaping unprompted.

### Analysis

**Cadmus strengths:**
- **NL shortcut is unbeatable when it works**: 0.3s for known patterns vs 3-10s LLM calls
- **Correct HTML/JS patterns**: Script placement, DOM readiness handled properly
- **File/code tasks are solid**: create, edit, search, build — all reliable

**Cadmus weaknesses:**
- **NL false positives are dangerous**: "show me processes" → file listing (wrong answer, high confidence). A wrong NL match that succeeds is worse than a failure that falls through to the LLM.
- **No OS awareness**: System prompt doesn't tell the LLM it's on macOS. The model defaults to Linux commands and wastes 3-5 attempts before finding macOS equivalents.
- **No desktop automation**: Can't open apps, interact with GUI, or run AppleScript. The tool set is file/code focused.
- **Aggressive NL matching**: Words like "show", "get", "check" are common verbs that match NL patterns even when the task is unrelated to files.

**Recommendations:**
1. Add OS detection to system prompt (`uname -s` → "You are on macOS. Use macOS commands.")
2. Add confidence threshold to NL shortcut — don't fire on single-verb matches
3. Consider `open` / `osascript` as tools for desktop automation
4. NL shortcut should not claim success for tasks that don't match semantically (process list ≠ file list)

## Reproducing

```bash
# Clean test directories
rm -rf /tmp/cad-test /tmp/pi-test && mkdir -p /tmp/cad-test /tmp/pi-test

# Run each test (example):
cd /tmp/cad-test && /path/to/cadmus --agent "list files"
cd /tmp/pi-test && pi --provider ollama --model "glm-4.7-flash:latest" -p "list files"

# Or use the automated script:
./evals/run_evals.sh
```

Note: Results will vary between runs due to LLM non-determinism. The NL
shortcut results (tests 1, 8) are deterministic.
