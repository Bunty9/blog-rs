use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "blog-rs-render", about = "Render a blog-rs markdown post to HTML")]
struct Cli {
    /// Input markdown file (use `-` for stdin)
    input: PathBuf,

    /// Emit asset manifest as JSON to this path
    #[arg(long)]
    assets_out: Option<PathBuf>,

    /// Emit frontmatter as YAML to this path
    #[arg(long)]
    frontmatter_out: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let src = if cli.input.as_os_str() == "-" {
        let mut s = String::new();
        if std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).is_err() {
            eprintln!("error: failed to read stdin");
            return ExitCode::from(2);
        }
        s
    } else {
        match std::fs::read_to_string(&cli.input) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: read {}: {e}", cli.input.display());
                return ExitCode::from(2);
            }
        }
    };

    let out = match content::render(&src) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("render error: {e}");
            return ExitCode::from(1);
        }
    };

    print!("{}", out.html);

    if let Some(p) = cli.assets_out {
        if let Err(e) = std::fs::write(&p, serde_json::to_vec_pretty(&out.assets).unwrap()) {
            eprintln!("error: write {}: {e}", p.display());
            return ExitCode::from(2);
        }
    }
    if let Some(p) = cli.frontmatter_out {
        if let Err(e) = std::fs::write(&p, serde_yaml::to_string(&out.frontmatter).unwrap()) {
            eprintln!("error: write {}: {e}", p.display());
            return ExitCode::from(2);
        }
    }

    ExitCode::SUCCESS
}
