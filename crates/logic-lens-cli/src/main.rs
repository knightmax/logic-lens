use clap::{Parser, ValueEnum};
use logic_lens_core::config::{Config, OutputFormat};
use logic_lens_core::diff::match_entities;
use logic_lens_core::entity::extract_entities;
use logic_lens_core::hallucination::check_hallucinated_imports;
use logic_lens_core::lint::{run_builtin_lenses, AuditLens, ChangeContext};
use logic_lens_core::output::{
    render_json, render_markdown, render_terminal, AuditResult, Verbosity,
};
use logic_lens_core::parser::parse_file;
use logic_lens_core::rules::load_all_rules;
use logic_lens_core::verify::run_verify;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "logic-lens",
    version,
    about = "Semantic auditor for AI-generated code"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser)]
enum Commands {
    /// Audit changes between an old and new file
    Audit {
        /// Path to the original file
        old: PathBuf,

        /// Path to the new/modified file
        new: PathBuf,

        /// Output format
        #[arg(long, default_value = "json")]
        format: CliFormat,

        /// Run local build verification after analysis
        #[arg(long)]
        verify: bool,

        /// Show only findings (no summary)
        #[arg(long, short)]
        quiet: bool,

        /// Show full details including timing
        #[arg(long, short)]
        verbose: bool,

        /// Path to config file (default: auto-discover logic-lens.toml)
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
enum CliFormat {
    Json,
    Terminal,
    Markdown,
}

impl From<CliFormat> for OutputFormat {
    fn from(f: CliFormat) -> Self {
        match f {
            CliFormat::Json => OutputFormat::Json,
            CliFormat::Terminal => OutputFormat::Terminal,
            CliFormat::Markdown => OutputFormat::Markdown,
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Audit {
            old,
            new,
            format,
            verify,
            quiet,
            verbose,
            config,
        } => {
            let config = match config {
                Some(path) => Config::from_file(&path).unwrap_or_else(|e| {
                    eprintln!("Error loading config: {}", e);
                    process::exit(2);
                }),
                None => Config::discover(&new),
            };

            let output_format: OutputFormat = format.into();

            if !old.exists() {
                eprintln!("Error: old file not found: {}", old.display());
                process::exit(2);
            }
            if !new.exists() {
                eprintln!("Error: new file not found: {}", new.display());
                process::exit(2);
            }

            let total_start = Instant::now();

            // Phase 1: Parse
            let parse_start = Instant::now();
            let old_parsed = parse_file(&old).unwrap_or_else(|e| {
                eprintln!("Error parsing old file: {}", e);
                process::exit(2);
            });
            let new_parsed = parse_file(&new).unwrap_or_else(|e| {
                eprintln!("Error parsing new file: {}", e);
                process::exit(2);
            });

            let old_entities =
                extract_entities(&old_parsed.source, &old_parsed.tree, old_parsed.language);
            let new_entities =
                extract_entities(&new_parsed.source, &new_parsed.tree, new_parsed.language);
            let parse_duration = parse_start.elapsed();

            // Phase 2: Diff
            let diff_start = Instant::now();
            let change_set = match_entities(
                &old_entities,
                &new_entities,
                &old_parsed.source,
                &new_parsed.source,
                new_parsed.language,
            );
            let diff_duration = diff_start.elapsed();

            // Phase 3: Analyze
            let analyze_start = Instant::now();
            let ctx = ChangeContext {
                old_source: &old_parsed.source,
                new_source: &new_parsed.source,
                old_entities: &old_entities,
                new_entities: &new_entities,
                change_set: &change_set,
                language: new_parsed.language,
                new_file_path: &new.display().to_string(),
            };

            let mut findings = run_builtin_lenses(&ctx, &config);

            // Load and run YAML rules
            let rules_dir = config
                .rules_dir
                .clone()
                .unwrap_or_else(|| new.parent().unwrap_or(&new).join(".logic-lens/rules"));
            let (yaml_lenses, rule_errors) = load_all_rules(&rules_dir);
            for err in &rule_errors {
                eprintln!("Warning: {}", err);
            }
            for lens in &yaml_lenses {
                findings.extend(lens.evaluate(&ctx));
            }

            // Hallucination detection
            let hallucination =
                check_hallucinated_imports(&new_parsed.source, new_parsed.language, &new);
            findings.extend(hallucination.findings);

            let analyze_duration = analyze_start.elapsed();
            let total_duration = total_start.elapsed();

            // Build result
            let result = AuditResult::build(
                &old.display().to_string(),
                &new.display().to_string(),
                &format!("{:?}", new_parsed.language),
                &old_entities,
                &new_entities,
                &change_set,
                findings,
                parse_duration,
                diff_duration,
                analyze_duration,
                total_duration,
            );

            // Output
            let output = match output_format {
                OutputFormat::Json => render_json(&result),
                OutputFormat::Terminal => {
                    let verbosity = if quiet {
                        Verbosity::Quiet
                    } else if verbose {
                        Verbosity::Verbose
                    } else {
                        Verbosity::Normal
                    };
                    let use_color = atty::is(atty::Stream::Stdout);
                    render_terminal(&result, verbosity, use_color)
                }
                OutputFormat::Markdown => render_markdown(&result),
            };
            print!("{}", output);

            // Optional verification
            if verify {
                let timeout = Duration::from_secs(config.verify.timeout.unwrap_or(120));
                let verify_result = run_verify(
                    config.verify.command.as_deref(),
                    new.parent().unwrap_or(&new),
                    timeout,
                );
                if verify_result.success {
                    eprintln!("Verification Passed");
                } else {
                    eprintln!("Verification Failed");
                    for line in &verify_result.error_lines {
                        eprintln!("  {}", line);
                    }
                }
            }

            // Exit code
            if result.has_errors() {
                process::exit(1);
            }
        }
    }
}
