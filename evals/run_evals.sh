#!/bin/bash
# Cadmus vs pi eval runner
# Usage: ./evals/run_evals.sh
# Requires: ollama running with glm-4.7-flash:latest, pi installed, cadmus built

set -e

CADMUS="$(cd "$(dirname "$0")/.." && pwd)/target/release/cadmus"
MODEL="glm-4.7-flash:latest"
CAD_DIR="/tmp/cad-test"
PI_DIR="/tmp/pi-test"

if [ ! -f "$CADMUS" ]; then
    echo "Build cadmus first: cargo build --release"
    exit 1
fi

if ! command -v pi &>/dev/null; then
    echo "pi not found. Install: npm install -g @mariozechner/pi-coding-agent"
    exit 1
fi

if ! curl -s http://localhost:11434/api/tags | grep -q "glm-4.7-flash"; then
    echo "Ollama not running or glm-4.7-flash not available."
    echo "Run: ollama pull glm-4.7-flash:latest"
    exit 1
fi

rm -rf "$CAD_DIR" "$PI_DIR"
mkdir -p "$CAD_DIR" "$PI_DIR"

RESULTS=""

run_test() {
    local name="$1"
    local task="$2"
    local timeout_s="${3:-30}"

    echo ""
    echo "═══════════════════════════════════════════════════"
    echo "  TEST: $name"
    echo "  TASK: $task"
    echo "═══════════════════════════════════════════════════"

    # Cadmus
    echo ""
    echo "--- CADMUS ---"
    cd "$CAD_DIR"
    local cad_start=$SECONDS
    local cad_out
    cad_out=$(timeout "$timeout_s" "$CADMUS" --agent "$task" 2>&1) || true
    local cad_time=$((SECONDS - cad_start))
    echo "$cad_out" | tail -5
    echo "  [${cad_time}s]"

    # pi
    echo ""
    echo "--- PI ---"
    cd "$PI_DIR"
    local pi_start=$SECONDS
    local pi_out
    pi_out=$(timeout "$timeout_s" pi --provider ollama --model "$MODEL" -p "$task" 2>&1) || true
    local pi_time=$((SECONDS - pi_start))
    echo "$pi_out" | tail -5
    echo "  [${pi_time}s]"

    RESULTS="$RESULTS\n| $name | ${cad_time}s | ${pi_time}s |"
}

# ── Tests ────────────────────────────────────────────────────

run_test "List files" "list files" 15

run_test "Create file" \
    "create a file called hello.txt with the text 'hello world'" 30

run_test "Replace text" \
    "replace 'world' with 'cadmus' in hello.txt" 30

run_test "Write + run Python" \
    "write a python script called fib.py that prints the first 20 fibonacci numbers, then run it" 45

run_test "Mkdir + move" \
    "create a folder called scripts and move fib.py into it" 30

run_test "HTML + JS project" \
    "create an index.html with a button that says Click Me, and a separate app.js that makes the button change color to red when clicked. Link the js from the html." 45

run_test "System report" \
    "get the current disk usage, memory usage, and CPU info and write a summary to system_report.txt" 60

run_test "Top 5 CPU processes" \
    "show me the top 5 processes using the most CPU right now" 30

run_test "Hostname + IP" \
    "what is my hostname and local IP address" 30

run_test "Battery status" \
    "check battery status" 30

run_test "Todo app" \
    "create a simple todo app: todo.html with an input field and Add button, and todo.js that adds items to a list below when you click Add. Keep it minimal." 45

# ── Summary ──────────────────────────────────────────────────

echo ""
echo "═══════════════════════════════════════════════════"
echo "  TIMING SUMMARY"
echo "═══════════════════════════════════════════════════"
echo ""
echo "| Test | Cadmus | pi |"
echo "|------|--------|-----|"
echo -e "$RESULTS"
echo ""
echo "Check outputs:"
echo "  Cadmus: $CAD_DIR/"
echo "  pi:     $PI_DIR/"
