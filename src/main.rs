use std::env;
use std::path::Path;

use std::time::Instant;
use std::process;

use cadmus::coding_strategy;

use cadmus::fs_strategy::FilesystemStrategy;
use cadmus::generic_planner::ExprLiteral;
use cadmus::pipeline;
use cadmus::type_expr::TypeExpr;
use cadmus::types::Goal;
use cadmus::ui;
use cadmus::plan;

const VERSION: &str = "v0.7.0";

fn main() {
    let args: Vec<String> = env::args().collect();

    // --plan <path> mode: load and execute a plan YAML file
    if let Some(pos) = args.iter().position(|a| a == "--plan") {
        let path = args.get(pos + 1).unwrap_or_else(|| {
            eprintln!("{}", ui::error("Missing plan path"));
            eprintln!();
            eprintln!("  {} cadmus --plan <path.yaml|path.sexp> [--dry-run]", ui::dim("usage:"));
            eprintln!();
            eprintln!("  {} cadmus --plan data/plans/find_pdfs.yaml", ui::dim("  $"));
            eprintln!("  {} cadmus --plan factorial.sexp", ui::dim("  $"));
            eprintln!("  {} cadmus --plan path.yaml --dry-run", ui::dim("  $"));
            process::exit(1);
        });

        let dry_run = args.iter().any(|a| a == "--dry-run");
        run_plan_mode(Path::new(path), dry_run);
        return;
    }

    // --agent mode: LLM agent loop with tool calling
    // With a task: single-shot mode (cadmus --agent "find bugs")
    // Without a task: interactive session (cadmus --agent)
    if args.iter().any(|a| a == "--agent") {
        let read_only = args.iter().any(|a| a == "--read-only");
        let pos = args.iter().position(|a| a == "--agent").unwrap();
        let maybe_task = args.get(pos + 1)
            .filter(|t| !t.starts_with("--"));

        if let Some(task) = maybe_task {
            // Single-shot mode
            run_agent_mode(task, read_only);
        } else {
            // Interactive session
            run_agent_session(read_only);
        }
        return;
    }

    // --tools mode: list available agent tools
    if args.iter().any(|a| a == "--tools") {
        let read_only = args.iter().any(|a| a == "--read-only");
        run_tools_mode(read_only);
        return;
    }

    // --demo mode: run strategy demos
    if args.iter().any(|a| a == "--demo") {
        run_demo_mode();
        return;
    }

    // Default: interactive chat mode
    let auto = args.iter().any(|a| a == "--auto");
    run_chat_mode(auto);
}

// ---------------------------------------------------------------------------
// Chat mode (NL UX)
// ---------------------------------------------------------------------------

fn run_chat_mode(auto: bool) {

    use cadmus::nl;
    use cadmus::nl::dialogue::DialogueState;
    use cadmus::line_editor::{LineEditor, ReadResult};

    println!();
    if auto {
        println!("{}", ui::banner("cadmus", VERSION, "reasoning inference engine — auto mode"));
    } else {
        println!("{}", ui::banner("cadmus", VERSION, "reasoning inference engine"));
    }
    println!();
    println!("  {} {}", ui::dim("try:"), ui::dim_white("zip up everything in ~/Downloads"));
    println!("  {}  {}", ui::dim("   "), ui::dim_white("find all PDFs in ~/Documents"));
    println!("  {}  {}", ui::dim("   "), ui::dim_white("what does walk_tree mean?"));
    println!("  {}  {}", ui::dim("   "), ui::dim_white("quit"));
    println!();

    let mut state = DialogueState::new();
    let mut editor = LineEditor::new();
    let prompt = format!("{}{}", ui::prompt(), ui::reset());

    loop {
        let input = match editor.read_line(&prompt) {
            ReadResult::Line(line) => line,
            ReadResult::Interrupted => {
                // Ctrl-C: just re-prompt
                println!();
                continue;
            }
            ReadResult::Eof => {
                // Ctrl-D: exit cleanly
                println!("{}", ui::dim("bye."));
                break;
            }
        };

        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if input == "quit" || input == "exit" || input == "q" {
            println!("{}", ui::dim("bye."));
            break;
        }

        // Add to history (only non-empty, non-quit commands)
        editor.add_history(input);

        let think_start = Instant::now();
        let response = nl::process_input(input, &mut state);
        let think_elapsed = think_start.elapsed();

        match response {
            nl::NlResponse::PlanCreated { plan_sexpr, summary: _, prompt: _ } => {
                if !auto {
                println!();
                println!("  {}", ui::status_ok("Plan created"));
                println!();
                println!("{}", ui::plan_block(&plan_sexpr));
                println!();
                println!("  {}", ui::dim("approve, edit, or reject?"));
                println!();
                } else {
                    // Auto mode: approve, codegen, and execute in one shot
                    use cadmus::calling_frame::{CallingFrame, DefaultFrame};

                    println!();
                    println!("  {}", ui::status_ok("Plan created"));
                    println!();
                    println!("{}", ui::plan_block(&plan_sexpr));
                    println!();

                    // Auto-approve: take the plan from dialogue state and codegen
                    if let Some(plan_def) = state.current_plan.take() {
                        let frame = DefaultFrame::from_plan(&plan_def);
                        match frame.codegen(&plan_def) {
                            Ok(script) => {

                                println!("  {}", ui::status_ok("Approved"));
                                println!();
                                println!("  {}", ui::subsection("Generated Racket Program"));
                                println!();
                                println!("{}", ui::code_block(&script));
                                println!();
                                println!("{}", ui::timing("reasoning", think_elapsed));
                                println!();

                                // Execute
                                println!("  {}", ui::status_active("Running..."));
                                let exec_start = Instant::now();
                                match frame.run_script(&script) {
                                    Ok(exec) => {
                                        let exec_elapsed = exec_start.elapsed();
                                        if !exec.stdout.is_empty() {
                                            println!();
                                            println!("{}", ui::code_block(&exec.stdout));
                                        }
                                        if !exec.stderr.is_empty() {
                                            eprintln!("{}", ui::dim(&exec.stderr));
                                        }
                                        println!();
                                        if exec.success {
                                            println!("  {}", ui::status_ok("Done"));
                                        } else {
                                            let code = exec.exit_code.unwrap_or(1);
                                            println!("  {}", ui::status_fail(&format!("Exit code {}", code)));
                                        }
                                        println!("{}", ui::timing("execution", exec_elapsed));
                                    }
                                    Err(e) => {
                                        let exec_elapsed = exec_start.elapsed();
                                        println!("  {}", ui::error(&format!("{}", e)));
                                        println!("  {}", ui::dim("Is Racket installed? Try: brew install racket"));
                                        println!("{}", ui::timing("execution", exec_elapsed));
                                    }
                                }
                            }
                            Err(e) => {
                                println!("  {}", ui::status_fail("Codegen failed"));
                                println!("  {}", ui::error(&format!("{}", e)));
                                println!("{}", ui::timing("reasoning", think_elapsed));
                            }
                        }
                    } else {
                        // Plan was created but not stored (shouldn't happen)
                        println!("  {}", ui::dim("(no plan available for auto-approve)"));
                        println!("{}", ui::timing("reasoning", think_elapsed));
                    }
                    println!();
                }
            }
            nl::NlResponse::PlanEdited { plan_sexpr, diff_description, .. } => {
                println!();
                println!("  {}", ui::status_info(&format!("Edited: {}", diff_description)));
                println!();
                println!("{}", ui::plan_block(&plan_sexpr));
                println!();
                println!("  {}", ui::dim("approve?"));
                println!();
            }
            nl::NlResponse::Explanation { text } => {
                println!();
                for line in text.lines() {
                    println!("  {}", ui::cyan(line));
                }
                println!();
            }
            nl::NlResponse::Approved { script } => {
                println!();
                println!("  {}", ui::status_ok("Approved"));
                if let Some(ref s) = script {
                    println!();
                    println!("  {}", ui::subsection("Generated Racket Program"));
                    println!();
                    println!("{}", ui::code_block(s));
                    println!();
                    let confirm_prompt = format!("  {} ", ui::dim("run this? (y/n)"));
                    let answer = match editor.read_line(&confirm_prompt) {
                        ReadResult::Line(line) => line.trim().to_lowercase(),
                        ReadResult::Interrupted | ReadResult::Eof => {
                            // Ctrl-C or Ctrl-D at confirmation = skip
                            println!();
                            String::new()
                        }
                    };
                    // Don't add confirmation to history
                    if answer == "y" || answer == "yes" {
                        println!();
                        println!("  {}", ui::status_active("Running..."));
                        use cadmus::calling_frame::{CallingFrame, DefaultFrame};
                        let frame = DefaultFrame::empty();
                        match frame.run_script(s) {
                            Ok(exec) => {
                                if !exec.stdout.is_empty() {
                                    println!();
                                    println!("{}", ui::code_block(&exec.stdout));
                                }
                                if !exec.stderr.is_empty() {
                                    eprintln!("{}", ui::dim(&exec.stderr));
                                }
                                println!();
                                if exec.success {
                                    println!("  {}", ui::status_ok("Done"));
                                } else {
                                    let code = exec.exit_code.unwrap_or(1);
                                    println!("  {}", ui::status_fail(&format!("Exit code {}", code)));
                                }
                            }
                            Err(e) => {
                                println!("  {}", ui::error(&format!("{}", e)));
                                println!("  {}", ui::dim("Is Racket installed? Try: brew install racket"));
                            }
                        }
                    } else {
                        println!("  {}", ui::dim("skipped."));
                    }
                } else {
                    println!("  {}", ui::dim("(could not compile to script)"));
                }
                println!();
            }
            nl::NlResponse::Rejected => {
                println!();
                println!("  {}", ui::dim("Plan discarded. Start fresh."));
                println!();
            }
            nl::NlResponse::NeedsClarification { needs } => {
                println!();
                for need in &needs {
                    println!("  {} {}", ui::yellow(ui::icon::DIAMOND), need);
                }
                println!();
            }
            nl::NlResponse::ParamSet { description, .. } => {
                println!();
                println!("  {}", ui::status_info(&description));
                println!();
            }
            nl::NlResponse::Error { message } => {
                println!();
                println!("  {}", ui::error(&message));
                println!();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Plan mode
// ---------------------------------------------------------------------------

fn run_plan_mode(path: &Path, dry_run: bool) {
    println!();
    println!("{}", ui::banner("cadmus", VERSION, &format!("Plan {} Racket", ui::icon::ARROW_RIGHT)));
    println!();

    // Load
    println!("  {}", ui::status_active("Loading"));
    let def = match plan::load_plan(path) {
        Ok(def) => def,
        Err(e) => {
            println!("  {}", ui::status_fail("Load failed"));
            eprintln!("  {}", ui::error(&format!("{}", e)));
            process::exit(1);
        }
    };

    println!("  {}", ui::kv("plan", &def.name));
    println!("  {}", ui::kv("source", &path.display().to_string()));
    for input in &def.inputs {
        if let Some(hint) = &input.type_hint {
            println!("  {}", ui::kv_dim(&input.name, hint));
        } else {
            println!("  {}", ui::kv_dim(&input.name, "(inferred)"));
        }
    }
    println!();

    // Show steps
    println!("  {}", ui::subsection(&format!("Steps ({})", def.steps.len())));
    for (i, s) in def.steps.iter().enumerate() {
        let args_str = match &s.args {
            plan::StepArgs::None => String::new(),
            plan::StepArgs::Scalar(v) => format!("{} {}", ui::icon::ARROW_RIGHT, v),
            plan::StepArgs::Map(m) => {
                let pairs: Vec<String> = m.iter()
                    .map(|(k, v)| match v {
                        plan::StepParam::Value(s) => format!("{}={}", k, s),
                        plan::StepParam::Steps(steps) => format!("{}=[{} sub-steps]", k, steps.len()),
                        plan::StepParam::Inline(step) => format!("{}={{{}}}", k, step.op),
                        plan::StepParam::Clauses(c) => format!("{}=[{} clauses]", k, c.len()),
                    })
                    .collect();
                format!("{{{}}}", pairs.join(", "))
            }
        };
        println!("{}", ui::step(i + 1, &s.op, &args_str));
    }
    println!();

    // Compile
    println!("  {}", ui::status_active("Compiling"));
    let registry = cadmus::fs_types::build_full_registry();
    let compiled = match plan::compile_plan(&def, &registry) {
        Ok(c) => c,
        Err(e) => {
            println!("  {}", ui::status_fail("Compile failed"));
            eprintln!("  {}", ui::error(&format!("{}", e)));
            process::exit(1);
        }
    };
    println!("  {}", ui::status_ok("Compiled"));
    println!();

    // Show compiled type chain
    println!("  {}", ui::subsection("Type Chain"));
    println!("  {}", ui::kv_dim("input", &compiled.input_type.to_string()));
    for cs in &compiled.steps {
        let type_info = format!("{} {} {}", cs.input_type, ui::icon::ARROW_RIGHT, cs.output_type);
        let is_map = cadmus::plan::step_needs_map(cs, &registry);
        if is_map {
            println!("{}", ui::step_each(cs.index + 1, &cs.op, &type_info));
        } else {
            println!("{}", ui::step(cs.index + 1, &cs.op, &type_info));
        }
        for (k, v) in &cs.params {
            println!("         {} {}", ui::dim(&format!("{}:", k)), ui::dim(v));
        }
    }
    println!("  {}", ui::kv_dim("output", &compiled.output_type.to_string()));
    println!();

    // Dry-run trace
    match plan::execute_plan(&compiled, &registry) {
        Ok(trace) => {
            println!("  {}", ui::subsection(&format!("Dry-Run Trace ({} steps)", trace.steps.len())));
            for ts in &trace.steps {
                let kind_tag = match ts.kind {
                    cadmus::fs_strategy::StepKind::Op => ui::dim("[op]"),
                    cadmus::fs_strategy::StepKind::Leaf => ui::dim("[input]"),
                    cadmus::fs_strategy::StepKind::Map => ui::dim("[map]"),
                    cadmus::fs_strategy::StepKind::Fold => ui::dim("[fold]"),
                };
                println!("{}", ui::step(ts.step, &ts.op_name, &format!("{} {}", kind_tag, ui::dim(&ts.command_hint))));
            }
            println!();
        }
        Err(e) => {
            println!("  {}", ui::warning(&format!("Trace: {}", e)));
            println!();
        }
    }

    // Generate Racket script
    println!("  {}", ui::subsection("Racket Script"));
    println!();

    use cadmus::calling_frame::{CallingFrame, DefaultFrame};
    let frame = DefaultFrame::from_plan(&def);
    let script = match frame.codegen(&def) {
        Ok(s) => s,
        Err(e) => {
            println!("  {}", ui::status_fail("Codegen failed"));
            eprintln!("  {}", ui::error(&format!("{}", e)));
            process::exit(1);
        }
    };

    println!("{}", ui::code_block(&script));
    println!();

    if !dry_run {
        println!("  {}", ui::status_active("Running..."));
        println!();

        match frame.run_script(&script) {
            Ok(exec) => {
                if !exec.stdout.is_empty() {
                    println!("{}", ui::code_block(&exec.stdout));
                }
                if !exec.stderr.is_empty() {
                    eprintln!("{}", ui::dim(&exec.stderr));
                }
                println!();
                if exec.success {
                    println!("  {}", ui::status_ok("Done"));
                } else {
                    let code = exec.exit_code.unwrap_or(1);
                    println!("  {}", ui::status_fail(&format!("Exit code {}", code)));
                    process::exit(code);
                }
            }
            Err(e) => {
                println!("  {}", ui::error(&format!("{}", e)));
                println!("  {}", ui::dim("Is Racket installed? Try: brew install racket"));
                process::exit(1);
            }
        }
    } else {
        println!("  {}", ui::dim("dry run complete — use without --dry-run to execute"));
    }
    println!();
}

// ---------------------------------------------------------------------------
// Demo mode
// ---------------------------------------------------------------------------

fn run_demo_mode() {
    println!();
    println!("{}", ui::banner("cadmus", VERSION, "Strategy Demo"));
    println!();

    // -----------------------------------------------------------------------
    // Strategy 1: Comparison
    // -----------------------------------------------------------------------
    println!("{}", ui::section("Strategy 1: Comparison"));
    println!();

    let goal = Goal {
        description: "Produce a structured comparison of Putin and Stalin as autocrats".into(),
        entities: vec!["putin".into(), "stalin".into()],
        fact_pack_paths: vec!["data/packs/facts/putin_stalin.facts.yaml".into()],
    };

    println!("{}", ui::kv("goal", &goal.description));
    println!("{}", ui::kv("entities", &goal.entities.join(", ")));
    println!("{}", ui::kv_dim("fact pack", &goal.fact_pack_paths.join(", ")));
    println!();

    match pipeline::run(&goal) {
        Ok(output) => {
            // Theory layer
            if !output.inferences.is_empty() || !output.conflicts.is_empty() {
                println!("  {}", ui::subsection("Theory Layer"));
                for inf in &output.inferences {
                    println!("{}", ui::inference_line(inf));
                }
                for c in &output.conflicts {
                    println!("{}", ui::conflict_line(c));
                }
                println!();
            }

            // Axes
            println!("  {}", ui::subsection(&format!("Comparison ({} axes)", output.axes.len())));
            println!();

            for axis in &output.axes {
                println!("{}", ui::axis_header(&axis.axis));

                for c in &axis.claims {
                    let entity = c.entity.as_deref().unwrap_or("?");
                    println!("{}", ui::claim(entity, &c.content));
                }

                for ev in &axis.evidence {
                    let entity = ev.entity.as_deref().unwrap_or("?");
                    for line in ev.content.lines() {
                        println!("{}", ui::evidence(entity, line));
                    }
                }

                for sim in &axis.similarities {
                    println!("{}", ui::similarity(&sim.content));
                }

                for con in &axis.contrasts {
                    for line in con.content.lines() {
                        println!("{}", ui::contrast_line(line));
                    }
                    if con.inferred {
                        println!("{}", ui::contrast_line(&ui::dim("[inferred]")));
                    }
                }

                for unc in &axis.uncertainties {
                    for line in unc.content.lines() {
                        println!("{}", ui::uncertainty(line));
                    }
                }

                if let Some(ref sum) = axis.summary {
                    for line in sum.content.lines() {
                        println!("{}", ui::summary_line(line));
                    }
                }

                for gap in &axis.gaps {
                    println!("{}", ui::gap_line(gap));
                }

                println!("{}", ui::axis_footer());
                println!();
            }

            let total_gaps: usize = output.axes.iter().map(|a| a.gaps.len()).sum();
            if total_gaps == 0 {
                println!("  {}", ui::status_ok("Comparison complete — all obligations fulfilled"));
            } else {
                println!("  {}", ui::status_warn(&format!("{} unfulfilled obligation(s)", total_gaps)));
            }
        }
        Err(e) => {
            println!("  {}", ui::status_fail("Comparison failed"));
            eprintln!("  {}", ui::error(&format!("{}", e)));
        }
    }

    println!();

    // -----------------------------------------------------------------------
    // Strategy 2: Coding
    // -----------------------------------------------------------------------
    println!("{}", ui::section("Strategy 2: Coding"));
    println!();

    let line_count = coding_strategy::EXAMPLE_LONG_FUNCTION.lines().count();
    println!("{}", ui::kv("goal", "Analyze code + plan extract-method refactoring"));
    println!("{}", ui::kv_dim("source", &format!("{} lines of Rust", line_count)));
    println!();

    match coding_strategy::run_coding(
        coding_strategy::EXAMPLE_LONG_FUNCTION,
        "Extract method to reduce function length",
    ) {
        Ok(output) => {
            // Source preview
            println!("  {}", ui::subsection("Source Preview"));
            let preview: String = output.source.lines().take(5)
                .enumerate()
                .map(|(i, l)| format!("  {} {} {}", ui::dim(&format!("{:>4}", i + 1)), ui::dim("│"), l))
                .collect::<Vec<_>>()
                .join("\n");
            println!("{}", preview);
            let remaining = output.source.lines().count().saturating_sub(5);
            if remaining > 0 {
                println!("  {}  {} {}", ui::dim("    "), ui::dim("│"), ui::dim(&format!("... {} more lines", remaining)));
            }
            println!();

            if !output.smells.is_empty() {
                println!("  {}", ui::subsection("Code Smells"));
                for (i, smell) in output.smells.iter().enumerate() {
                    if i == output.smells.len() - 1 {
                        println!("{}", ui::tree_last(smell));
                    } else {
                        println!("{}", ui::tree_item(smell));
                    }
                }
                println!();
            }

            if !output.refactorings.is_empty() {
                println!("  {}", ui::subsection("Planned Refactorings"));
                for (i, r) in output.refactorings.iter().enumerate() {
                    if i == output.refactorings.len() - 1 {
                        println!("{}", ui::tree_last(r));
                    } else {
                        println!("{}", ui::tree_item(r));
                    }
                }
                println!();
            }

            if !output.type_info.is_empty() {
                println!("  {}", ui::subsection("Type Information"));
                for (i, info) in output.type_info.iter().enumerate() {
                    if i == output.type_info.len() - 1 {
                        println!("{}", ui::tree_last(info));
                    } else {
                        println!("{}", ui::tree_item(info));
                    }
                }
                println!();
            }

            if !output.tests.is_empty() {
                println!("  {}", ui::subsection("Generated Tests"));
                for test in &output.tests {
                    println!("{}", ui::code_block(test));
                    println!();
                }
            }

            println!("  {}", ui::status_ok("Coding analysis complete"));
        }
        Err(e) => {
            println!("  {}", ui::status_fail("Coding analysis failed"));
            eprintln!("  {}", ui::error(&format!("{}", e)));
        }
    }

    println!();

    // -----------------------------------------------------------------------
    // Strategy 3: Filesystem (dry-run)
    // -----------------------------------------------------------------------
    println!("{}", ui::section("Strategy 3: Filesystem"));
    println!();

    // Goal A: CBZ extraction
    println!("{}", ui::kv("goal", "Extract images from a CBZ comic archive"));
    println!();

    let fs_target_a = TypeExpr::seq(TypeExpr::entry(
        TypeExpr::prim("Name"),
        TypeExpr::file(TypeExpr::prim("Image")),
    ));
    let fs_available_a = vec![
        ExprLiteral::new(
            "comic.cbz",
            TypeExpr::file(TypeExpr::archive(
                TypeExpr::file(TypeExpr::prim("Image")),
                TypeExpr::prim("Cbz"),
            )),
            "my_comic.cbz",
        ),
    ];

    let strategy_a = FilesystemStrategy::new();
    match strategy_a.dry_run(fs_target_a, fs_available_a) {
        Ok(trace) => {
            print_trace(&trace);
        }
        Err(e) => {
            println!("  {}", ui::status_fail("Planning failed"));
            eprintln!("  {}", ui::error(&format!("{}", e)));
        }
    }

    // Goal B: List directory
    println!("{}", ui::kv("goal", "List directory contents"));
    println!();

    let strategy = FilesystemStrategy::new();
    match strategy.dry_run(
        TypeExpr::seq(TypeExpr::entry(
            TypeExpr::prim("Name"),
            TypeExpr::prim("Bytes"),
        )),
        vec![ExprLiteral::new(
            "/home/user/docs",
            TypeExpr::dir(TypeExpr::prim("Bytes")),
            "/home/user/docs",
        )],
    ) {
        Ok(trace) => {
            print_trace(&trace);
        }
        Err(e) => {
            println!("  {}", ui::status_fail("Planning failed"));
            eprintln!("  {}", ui::error(&format!("{}", e)));
        }
    }

    println!("{}", ui::rule());
    println!("  {}", ui::status_ok("All strategies complete"));
    println!();
}

// ---------------------------------------------------------------------------
// Agent interactive session
// ---------------------------------------------------------------------------

fn run_agent_session(read_only: bool) {
    use cadmus::agent::{AgentConfig, run_agent};
    use cadmus::line_editor::{LineEditor, ReadResult};

    println!();
    println!("{}", ui::banner("cadmus", VERSION, "agent session"));
    println!();

    let config = AgentConfig {
        read_only,
        ..AgentConfig::default()
    };

    if read_only {
        println!("  {} read-only mode (write ops disabled)", ui::dim("mode:"));
    }
    println!("  {} {}  {} {}",
        ui::dim("llm:"), config.llm_url,
        ui::dim("model:"), config.model,
    );
    println!();
    println!("  {} {}", ui::dim("type a task, or"), ui::dim_white("quit"));
    println!();

    let mut editor = LineEditor::new();
    let prompt = format!("{}{}", ui::agent_prompt(), ui::reset());

    loop {
        let input = match editor.read_line(&prompt) {
            ReadResult::Line(line) => line,
            ReadResult::Interrupted => {
                println!();
                continue;
            }
            ReadResult::Eof => {
                println!("{}", ui::dim("bye."));
                break;
            }
        };

        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if input == "quit" || input == "exit" || input == "q" {
            println!("{}", ui::dim("bye."));
            break;
        }

        let start = std::time::Instant::now();
        let result = run_agent(input, &config);
        let elapsed = start.elapsed();

        println!();
        if result.completed {
            println!("  {}", ui::status_ok(&format!(
                "Completed in {} tool call(s), {:.1}s",
                result.tool_calls,
                elapsed.as_secs_f64(),
            )));
        } else {
            println!("  {}", ui::status_warn(&format!(
                "Stopped after {} tool call(s), {:.1}s",
                result.tool_calls,
                elapsed.as_secs_f64(),
            )));
        }
        println!();
        println!("{}", result.summary);
        println!();
    }
}

// ---------------------------------------------------------------------------
// Agent single-shot mode
// ---------------------------------------------------------------------------

fn run_agent_mode(task: &str, read_only: bool) {
    use cadmus::agent::{AgentConfig, run_agent};

    println!();
    println!("{}", ui::banner("cadmus", VERSION, "agent mode"));
    println!();
    println!("  {} {}", ui::dim("task:"), task);
    if read_only {
        println!("  {} read-only mode (write ops disabled)", ui::dim("mode:"));
    }
    println!();

    let config = AgentConfig {
        read_only,
        ..AgentConfig::default()
    };

    println!("  {} {}  {} {}",
        ui::dim("llm:"), config.llm_url,
        ui::dim("model:"), config.model,
    );

    let start = std::time::Instant::now();
    let result = run_agent(task, &config);
    let elapsed = start.elapsed();

    println!();
    if result.completed {
        println!("  {}", ui::status_ok(&format!(
            "Completed in {} tool call(s), {:.1}s",
            result.tool_calls,
            elapsed.as_secs_f64(),
        )));
    } else {
        println!("  {}", ui::status_warn(&format!(
            "Stopped after {} tool call(s), {:.1}s",
            result.tool_calls,
            elapsed.as_secs_f64(),
        )));
    }
    println!();
    println!("{}", result.summary);
    println!();
}

// ---------------------------------------------------------------------------
// Tools listing mode
// ---------------------------------------------------------------------------

fn run_tools_mode(read_only: bool) {
    use cadmus::tools;

    println!();
    println!("{}", ui::banner("cadmus", VERSION, "available agent tools"));
    println!();

    let defs = tools::tool_definitions(read_only);
    for tool in &defs {
        let name = tool["function"]["name"].as_str().unwrap_or("?");
        let desc = tool["function"]["description"].as_str().unwrap_or("");
        let params = tool["function"]["parameters"]["required"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let is_write = tools::is_write_op(name);
        let tag = if is_write { " [write]" } else { "" };
        println!("  {} {}({}){}",
            ui::dim("▸"),
            name,
            ui::dim(&params),
            if is_write { ui::yellow(tag) } else { String::new() },
        );
        println!("    {}", ui::dim(desc));
    }
    println!();
    println!("  {} {} tools available", ui::dim("total:"), defs.len());
    if read_only {
        println!("  {} write ops excluded (--read-only)", ui::dim("note:"));
    }
    println!();
}

/// Print a DryRunTrace with rich formatting.
fn print_trace(trace: &cadmus::fs_strategy::DryRunTrace) {
    println!("  {}", ui::kv_dim("target", &trace.goal.to_string()));
    for ts in &trace.steps {
        let kind_tag = match ts.kind {
            cadmus::fs_strategy::StepKind::Op => "op",
            cadmus::fs_strategy::StepKind::Leaf => "input",
            cadmus::fs_strategy::StepKind::Map => "map",
            cadmus::fs_strategy::StepKind::Fold => "fold",
        };
        let detail = format!(
            "{} {} {}",
            ui::dim(&format!("[{}]", kind_tag)),
            ui::dim(&ts.command_hint),
            if ts.inputs.is_empty() { String::new() } else { ui::dim(&format!("{} {}", ui::icon::ARROW_LEFT, ts.inputs.join(", "))) },
        );
        println!("{}", ui::step(ts.step, &ts.op_name, &detail));
    }
    println!("  {}", ui::kv_dim("output", &trace.steps.last().map(|s| s.output.as_str()).unwrap_or("?")));
    println!();
}
