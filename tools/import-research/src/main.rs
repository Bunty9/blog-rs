//! Convert `gemini-research.md` (or any compatible source) into a directory of
//! per-domain markdown files with frontmatter, shortcode-wrapped code, and
//! callout summaries. Idempotent — overwriting outputs in place.

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

use import_research::{parse, emit};

#[derive(Parser)]
#[command(name = "import-research", about = "Convert research markdown into seed articles")]
struct Cli {
    /// Path to the input markdown file (e.g. ../gemini-research.md)
    #[arg(short, long, default_value = "../gemini-research.md")]
    input: PathBuf,

    /// Output directory for one .md file per domain
    #[arg(short, long, default_value = "content/articles")]
    out: PathBuf,

    /// Print what would be written without touching the filesystem
    #[arg(long)]
    dry_run: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let src = match std::fs::read_to_string(&cli.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: read {}: {e}", cli.input.display());
            return ExitCode::from(2);
        }
    };

    let domains = match parse::split_domains(&src) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("parse error: {e}");
            return ExitCode::from(1);
        }
    };

    if domains.is_empty() {
        eprintln!("no `## **Domain N: ...**` sections found");
        return ExitCode::from(1);
    }

    if !cli.dry_run {
        if let Err(e) = std::fs::create_dir_all(&cli.out) {
            eprintln!("error: mkdir {}: {e}", cli.out.display());
            return ExitCode::from(2);
        }
    }

    for d in &domains {
        let article = emit::to_article(d);
        let path = cli.out.join(format!("domain-{}-{}.md", d.number, d.slug));
        if cli.dry_run {
            println!("--- {} ---\n{article}", path.display());
        } else if let Err(e) = std::fs::write(&path, article) {
            eprintln!("error: write {}: {e}", path.display());
            return ExitCode::from(2);
        } else {
            println!("wrote {} ({} bytes body)", path.display(), d.body.len());
        }
    }

    ExitCode::SUCCESS
}
