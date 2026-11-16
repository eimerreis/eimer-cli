//! Renders a document YAML with Typst: the YAML's `type:` picks `templates/<type>.typ`.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use super::assets;
use crate::core::error::HintExt;
use crate::core::{config, process};

pub const TYPES: &[&str] = &[
    "invoice",
    "quote",
    "letter",
    "reminder",
    "contract",
    "time-report",
];

pub struct RenderOptions<'a> {
    pub data: &'a Path,
    pub output: Option<&'a Path>,
    pub sender: Option<&'a Path>,
    pub watch: bool,
    pub png: bool,
}

/// Renders the document and returns the output path (`-{p}` is the page number for PNGs).
pub fn render(options: &RenderOptions) -> Result<PathBuf> {
    let yaml = std::fs::read_to_string(options.data)
        .with_context(|| format!("Failed to read {}", options.data.display()))?;
    let kind = document_type(&yaml)?;
    let sender = match options.sender {
        Some(path) => path.to_path_buf(),
        None => default_sender()?,
    };
    if !sender.exists() {
        return Err(anyhow!("No sender details at {}", sender.display()))
            .hint("Create them with `eimer doc new sender`, then fill in your business details");
    }
    let assets = assets::unpack()?;
    let output = output_path(options);
    let args = typst_args(kind, options, &sender, &output, &assets)?;

    let code = process::run_inherit(&args)
        .hint("Install Typst 0.15+: https://github.com/typst/typst#installation")?;
    if code != 0 {
        bail!("typst exited with {code}");
    }
    Ok(output)
}

/// `~/.config/eimer/sender.yaml`.
pub fn default_sender() -> Result<PathBuf> {
    Ok(config::config_dir()?.join("sender.yaml"))
}

/// The top-level `type:` of a document YAML.
pub fn document_type(yaml: &str) -> Result<&'static str> {
    let value = yaml
        .lines()
        .find_map(|line| line.strip_prefix("type:"))
        .map(|rest| {
            let rest = rest.split(" #").next().unwrap_or_default();
            rest.trim().trim_matches(['"', '\''])
        })
        .ok_or_else(|| anyhow!("The document has no top-level \"type:\" field"))?;
    TYPES
        .iter()
        .find(|t| **t == value)
        .copied()
        .ok_or_else(|| anyhow!("Unknown document type \"{value}\""))
        .hint(format!("Known types: {}", TYPES.join(", ")))
}

fn output_path(options: &RenderOptions) -> PathBuf {
    if let Some(output) = options.output {
        return output.to_path_buf();
    }
    let stem = options
        .data
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let name = if options.png {
        format!("{stem}-{{p}}.png")
    } else {
        format!("{stem}.pdf")
    };
    options.data.with_file_name(name)
}

/// `typst compile|watch` arguments. The project root is the filesystem root, so the data and
/// sender files can live anywhere; Typst addresses them as root-relative paths.
fn typst_args(
    kind: &str,
    options: &RenderOptions,
    sender: &Path,
    output: &Path,
    assets: &Path,
) -> Result<Vec<String>> {
    let data = absolute(options.data)?;
    let root = fs_root(&data);
    let mut args = vec![
        "typst".to_string(),
        if options.watch { "watch" } else { "compile" }.to_string(),
        "--root".to_string(),
        root.display().to_string(),
        "--font-path".to_string(),
        assets.join("brand").join("fonts").display().to_string(),
        "--ignore-system-fonts".to_string(),
        "--input".to_string(),
        format!("data={}", root_relative(&data, &root)),
        "--input".to_string(),
        format!("sender={}", root_relative(&absolute(sender)?, &root)),
    ];
    if options.png {
        args.extend(["--ppi".to_string(), "144".to_string()]);
    } else {
        args.extend(["--pdf-standard".to_string(), "a-3b".to_string()]);
    }
    args.push(
        assets
            .join("templates")
            .join(format!("{kind}.typ"))
            .display()
            .to_string(),
    );
    args.push(absolute(output)?.display().to_string());
    Ok(args)
}

fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).with_context(|| format!("Failed to resolve {}", path.display()))
}

/// `/`, or the drive (`C:\`) on Windows.
fn fs_root(path: &Path) -> PathBuf {
    path.ancestors().last().unwrap_or(path).to_path_buf()
}

/// `/home/me/a.yaml` for Typst, which joins absolute paths onto the project root.
fn root_relative(path: &Path, root: &Path) -> String {
    let parts: Vec<String> = path
        .strip_prefix(root)
        .unwrap_or(path)
        .components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    format!("/{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_document_type() {
        assert_eq!(
            document_type("type: invoice\nnumber: 1").unwrap(),
            "invoice"
        );
        assert_eq!(
            document_type("# c\ntype: \"time-report\" # note\n").unwrap(),
            "time-report"
        );
    }

    #[test]
    fn rejects_missing_or_unknown_types() {
        assert!(document_type("number: 1").is_err());
        assert!(
            document_type("  type: invoice").is_err(),
            "nested key is not the type"
        );
        assert!(document_type("type: memo").is_err());
    }

    #[test]
    fn defaults_output_next_to_the_data() {
        let data = Path::new("work/RE-1.yaml");
        let options = |png| RenderOptions {
            data,
            output: None,
            sender: None,
            watch: false,
            png,
        };
        assert_eq!(output_path(&options(false)), Path::new("work/RE-1.pdf"));
        assert_eq!(output_path(&options(true)), Path::new("work/RE-1-{p}.png"));
    }

    #[cfg(unix)]
    #[test]
    fn builds_typst_arguments() {
        let options = RenderOptions {
            data: Path::new("/docs/RE-1.yaml"),
            output: None,
            sender: None,
            watch: false,
            png: false,
        };
        let args = typst_args(
            "invoice",
            &options,
            Path::new("/cfg/sender.yaml"),
            Path::new("/docs/RE-1.pdf"),
            Path::new("/cache/documents"),
        )
        .unwrap();
        assert_eq!(
            args,
            [
                "typst",
                "compile",
                "--root",
                "/",
                "--font-path",
                "/cache/documents/brand/fonts",
                "--ignore-system-fonts",
                "--input",
                "data=/docs/RE-1.yaml",
                "--input",
                "sender=/cfg/sender.yaml",
                "--pdf-standard",
                "a-3b",
                "/cache/documents/templates/invoice.typ",
                "/docs/RE-1.pdf",
            ]
        );
    }
}
