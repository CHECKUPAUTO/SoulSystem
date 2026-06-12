use scirust_autodiff::Dual;
use scirust_bridge::Pipeline;
use std::io::{self, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "--repl" {
        run_repl();
        return;
    }

    let input = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        eprintln!("Usage: scirust-cli [OPTIONS] \"<expression>\"");
        eprintln!("Options:");
        eprintln!("  --repl          Start interactive REPL mode");
        eprintln!("Examples:");
        eprintln!("  scirust-cli \"x^2 + 2*x + 1\"");
        eprintln!("  scirust-cli \"dérivée de x^2\"");
        eprintln!("  scirust-cli \"solve x^2 - 5*x + 6 = 0\"");
        eprintln!("  scirust-cli \"prove x + x = 2*x\"");
        std::process::exit(1);
    };

    run_once(&input);
}

fn run_once(input: &str) {
    eprintln!("[scirust] === SciRust Mathematical Intelligence Pipeline ===");
    eprintln!("[scirust] Input: {}", input);

    let mut pipeline = Pipeline::new();

    match pipeline.run(input) {
        Ok(out) => {
            print_output(&out);

            if out.action != "prove" {
                eprintln!("[scirust] --- Dual Number Verification ---");
                let x_dual = Dual::var(2.0);
                let result = evaluate_with_dual(&out.simplified, "x", x_dual);
                eprintln!("[scirust] Dual eval: value={}, deriv={}", result.value, result.deriv);
            }
            eprintln!("[scirust] === Pipeline complete ===");
        }
        Err(e) => {
            eprintln!("[scirust] Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_repl() {
    println!("SciRust REPL — Type expressions or 'quit' to exit.");
    println!("Examples: x^2 + 1 | dérivée de sin(x) | solve x^2 - 4 = 0");

    let mut pipeline = Pipeline::new();

    loop {
        print!("> ");
        io::stdout().flush().unwrap();
        let mut line = String::new();
        match io::stdin().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "quit" || trimmed == "exit" {
                    println!("Au revoir !");
                    break;
                }
                if trimmed.starts_with("var ") {
                    // Set variable: var x = 3.5
                    let parts: Vec<&str> = trimmed[4..].split('=').collect();
                    if parts.len() == 2 {
                        let name = parts[0].trim();
                        if let Ok(val) = parts[1].trim().parse::<f64>() {
                            pipeline.vars.insert(name.to_string(), val);
                            println!("Set {} = {}", name, val);
                        }
                    }
                    continue;
                }
                if trimmed == "vars" {
                    for (k, v) in &pipeline.vars {
                        println!("  {} = {}", k, v);
                    }
                    continue;
                }
                if trimmed == "patterns" {
                    let best = pipeline.memory.best_patterns(5);
                    for (pat, score) in best {
                        println!("  {} (score: {:.2})", pat, score);
                    }
                    continue;
                }
                match pipeline.run(trimmed) {
                    Ok(out) => {
                        print_output(&out);
                    }
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
                break;
            }
        }
    }
}

fn print_output(out: &scirust_bridge::PipelineOutput) {
    println!("[parsed]    {}", out.parsed);
    println!("[simplified] {}", out.simplified);
    if !out.derivative.is_empty() {
        println!("[derivative] {}", out.derivative);
    }
    if !out.solution.is_empty() {
        println!("[solution]  {}", out.solution);
    }
    if out.action != "prove" {
        println!("[evaluated] {}", out.value);
        println!("[rust code] ---");
        println!("{}", out.rust_code);
    }
}

/// Evaluate a symbolic expression by interpreting it with Dual numbers.
fn evaluate_with_dual(expr: &str, var: &str, val: Dual) -> Dual {
    match scirust_symbolic::parse(expr) {
        Ok(ast) => eval_dual(&ast, var, val),
        Err(_) => Dual::new(0.0, 0.0),
    }
}

fn eval_dual(expr: &scirust_symbolic::Expr, var: &str, val: Dual) -> Dual {
    use scirust_symbolic::Expr;
    match expr {
        Expr::Const(c) => Dual::primal(*c),
        Expr::Var(name) => {
            if name == var { val } else { Dual::primal(0.0) }
        }
        Expr::Add(a, b) => eval_dual(a, var, val) + eval_dual(b, var, val),
        Expr::Sub(a, b) => eval_dual(a, var, val) - eval_dual(b, var, val),
        Expr::Mul(a, b) => eval_dual(a, var, val) * eval_dual(b, var, val),
        Expr::Div(a, b) => eval_dual(a, var, val) / eval_dual(b, var, val),
        Expr::Neg(a) => -eval_dual(a, var, val),
        Expr::Pow(a, b) => {
            if let Expr::Const(n) = &**b {
                eval_dual(a, var, val).powi(*n as i32)
            } else {
                eval_dual(a, var, val).powf(eval_dual(b, var, val).value)
            }
        }
        Expr::Sin(a) => eval_dual(a, var, val).sin(),
        Expr::Cos(a) => eval_dual(a, var, val).cos(),
        Expr::Exp(a) => eval_dual(a, var, val).exp(),
        Expr::Ln(a) => eval_dual(a, var, val).ln(),
        Expr::Sqrt(a) => eval_dual(a, var, val).sqrt(),
        Expr::Abs(a) => eval_dual(a, var, val).abs(),
    }
}
