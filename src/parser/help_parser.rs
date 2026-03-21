use regex::Regex;

use crate::error::Result;
use crate::registry::models::{ParamType, ToolParam};

pub trait HelpParser: Send + Sync {
    fn parse(&self, help_text: &str) -> Result<Vec<ToolParam>>;
}

#[derive(Default)]
pub struct HeuristicHelpParser;

impl HeuristicHelpParser {
    pub fn new() -> Self {
        Self
    }

    /// Extract the tool description from help text.
    /// Returns the first non-empty line that doesn't start with "usage:" and
    /// appears before any flags section header.
    pub fn extract_description(&self, help_text: &str) -> Option<String> {
        let section_headers = [
            "options:",
            "arguments:",
            "flags:",
            "parameters:",
            "available options:",
        ];

        for line in help_text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let lower = trimmed.to_lowercase();

            // Stop if we hit a section header
            if section_headers
                .iter()
                .any(|h| lower == *h || lower.ends_with(h))
            {
                return None;
            }

            // Skip usage lines and their continuations
            if lower.starts_with("usage:") || lower.starts_with("usage ") {
                continue;
            }

            // Skip lines that look like usage continuations (contain [--flag] patterns)
            if trimmed.contains("[--") || trimmed.contains("[-") {
                continue;
            }

            // Skip lines that look like flags
            if trimmed.starts_with('-') {
                continue;
            }

            // Skip error messages (e.g., "ls: unrecognized option '--help'")
            if lower.contains("unrecognized option")
                || lower.contains("unknown option")
                || lower.contains("invalid option")
                || lower.contains("illegal option")
                || lower.contains("error:")
            {
                continue;
            }

            return Some(trimmed.to_string());
        }

        None
    }

    fn infer_param_type(type_token: &str) -> ParamType {
        match type_token.to_uppercase().as_str() {
            "FILE" | "PATH" | "STRING" | "STR" | "TEXT" | "DIR" | "NAME" | "URL" | "PATTERN" => {
                ParamType::String
            }
            "INT" | "NUM" | "NUMBER" | "N" | "COUNT" | "SIZE" | "PORT" => ParamType::Integer,
            "FLOAT" | "DECIMAL" => ParamType::Float,
            "VALUE" => ParamType::Float,
            _ => ParamType::String, // Default to String for unknown types
        }
    }
}

impl HelpParser for HeuristicHelpParser {
    fn parse(&self, help_text: &str) -> Result<Vec<ToolParam>> {
        let mut params = Vec::new();

        // Regex to match flag lines:
        //   [-s,] --long-name [<TYPE>]    Description text [default: X]
        //   [-s,] --long-name [TYPE]      Description text [default: X]  (bare metavar, e.g. argparse)
        let flag_re = Regex::new(
            r"^\s{0,8}(?:-\w,?\s+)?--(?P<name>[\w][\w-]*)(?:\s+(?:<(?P<type>[A-Za-z_]+)>|(?P<baretype>[A-Z][A-Z_]*)))?(?:\s{2,}|\s*$)(?P<desc>.*)?$"
        ).expect("Invalid regex");

        let default_re = Regex::new(r"\[default:\s*([^\]]+)\]").expect("Invalid regex");
        let required_re = Regex::new(r"\[required\]").expect("Invalid regex");

        for line in help_text.lines() {
            if let Some(caps) = flag_re.captures(line) {
                let name = match caps.name("name") {
                    Some(m) => m.as_str().to_string(),
                    None => continue,
                };
                let type_token = caps
                    .name("type")
                    .or_else(|| caps.name("baretype"))
                    .map(|m| m.as_str().to_string());
                let desc_raw = caps
                    .name("desc")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                // Determine param type
                let param_type = match &type_token {
                    Some(t) => Self::infer_param_type(t),
                    None => ParamType::Boolean,
                };

                // Check for [default: X]
                let default_value = default_re
                    .captures(&desc_raw)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().trim().to_string());

                // Check for [required]
                let has_required = required_re.is_match(&desc_raw);

                // Clean up description: remove [default: X] and [required] markers
                let mut description = desc_raw.clone();
                if let Some(ref dv) = default_value {
                    description = description.replace(&format!("[default: {}]", dv), "");
                }
                let description = description.replace("[required]", "").trim().to_string();

                let required = has_required;

                // Skip --help and --version
                if name == "help" || name == "version" {
                    continue;
                }

                params.push(ToolParam {
                    name,
                    description,
                    param_type,
                    required,
                    default_value,
                });
            }
        }

        Ok(params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> HeuristicHelpParser {
        HeuristicHelpParser::new()
    }

    #[test]
    fn test_parse_required_flag() {
        let help = "  --output <FILE>  Output path [required]";
        let params = parser().parse(help).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "output");
        assert_eq!(params[0].param_type, ParamType::String);
        assert!(params[0].required);
        assert!(params[0].default_value.is_none());
    }

    #[test]
    fn test_parse_default_value() {
        let help = "  --width <INT>  Width [default: 800]";
        let params = parser().parse(help).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "width");
        assert_eq!(params[0].param_type, ParamType::Integer);
        assert!(!params[0].required);
        assert_eq!(params[0].default_value.as_deref(), Some("800"));
    }

    #[test]
    fn test_parse_boolean_flag() {
        let help = "  --verbose  Enable verbose output";
        let params = parser().parse(help).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "verbose");
        assert_eq!(params[0].param_type, ParamType::Boolean);
        assert!(!params[0].required);
        assert!(params[0].default_value.is_none());
    }

    #[test]
    fn test_parse_required_with_default() {
        let help = "  --mode <STR>  Mode [required] [default: fast]";
        let params = parser().parse(help).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "mode");
        assert_eq!(params[0].param_type, ParamType::String);
        assert!(params[0].required); // required wins
        assert_eq!(params[0].default_value.as_deref(), Some("fast"));
    }

    #[test]
    fn test_parse_short_and_long_flag() {
        let help = "  -o, --output <FILE>  Output path";
        let params = parser().parse(help).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "output");
        assert_eq!(params[0].param_type, ParamType::String);
    }

    #[test]
    fn test_parse_multiple_flags() {
        let help = "\
Options:
  --input <FILE>   Input file [required]
  --output <FILE>  Output file
  --width <INT>    Width [default: 800]
  --verbose        Verbose mode";
        let params = parser().parse(help).unwrap();
        assert_eq!(params.len(), 4);
        assert_eq!(params[0].name, "input");
        assert!(params[0].required);
        assert_eq!(params[1].name, "output");
        assert_eq!(params[2].name, "width");
        assert_eq!(params[3].name, "verbose");
        assert_eq!(params[3].param_type, ParamType::Boolean);
    }

    #[test]
    fn test_skip_help_and_version() {
        let help = "\
  --input <FILE>  Input file
  --help           Show help
  --version        Show version";
        let params = parser().parse(help).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "input");
    }

    #[test]
    fn test_parse_empty_help() {
        let help = "";
        let params = parser().parse(help).unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn test_parse_no_flags() {
        let help = "Usage: tool <INPUT> <OUTPUT>\n\nSome description text.";
        let params = parser().parse(help).unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn test_type_inference() {
        let help = "\
  --path <PATH>      Path
  --count <COUNT>    Count
  --rate <FLOAT>     Rate
  --amount <DECIMAL>  Amount
  --val <VALUE>      Value
  --num <NUM>        Num
  --n <N>            N";
        let params = parser().parse(help).unwrap();
        assert_eq!(params[0].param_type, ParamType::String); // PATH
        assert_eq!(params[1].param_type, ParamType::Integer); // COUNT
        assert_eq!(params[2].param_type, ParamType::Float); // FLOAT
        assert_eq!(params[3].param_type, ParamType::Float); // DECIMAL
        assert_eq!(params[4].param_type, ParamType::Float); // VALUE
        assert_eq!(params[5].param_type, ParamType::Integer); // NUM
        assert_eq!(params[6].param_type, ParamType::Integer); // N
    }

    #[test]
    fn test_parse_argparse_style_bare_metavar() {
        // Python argparse uses bare uppercase metavars: --message MESSAGE
        let help = "\
usage: tool [-h] --message MESSAGE [--repeat REPEAT] [--uppercase]

Echo a message to stdout

options:
  -h, --help         show this help message and exit
  --message MESSAGE  Message to echo [required]
  --repeat REPEAT    Number of times to repeat [default: 1]
  --uppercase        Convert to uppercase";

        let params = parser().parse(help).unwrap();
        assert_eq!(params.len(), 3);

        let message = params.iter().find(|p| p.name == "message").unwrap();
        assert_eq!(message.param_type, ParamType::String);
        assert!(message.required);

        let repeat = params.iter().find(|p| p.name == "repeat").unwrap();
        assert_eq!(repeat.param_type, ParamType::String); // REPEAT is unknown, defaults to String
        assert!(!repeat.required);
        assert_eq!(repeat.default_value.as_deref(), Some("1"));

        let uppercase = params.iter().find(|p| p.name == "uppercase").unwrap();
        assert_eq!(uppercase.param_type, ParamType::Boolean);
    }

    #[test]
    fn test_extract_description() {
        let help = "Resize an image to given dimensions\n\nUsage: resize [OPTIONS]\n\nOptions:\n  --width <INT>  Width";
        let desc = parser().extract_description(help);
        assert_eq!(desc.as_deref(), Some("Resize an image to given dimensions"));
    }

    #[test]
    fn test_extract_description_skips_usage() {
        let help = "Usage: tool [OPTIONS]\n\nA great tool for things\n\nOptions:";
        let desc = parser().extract_description(help);
        assert_eq!(desc.as_deref(), Some("A great tool for things"));
    }

    #[test]
    fn test_extract_description_none_if_only_flags() {
        let help = "Options:\n  --verbose  Verbose mode";
        let desc = parser().extract_description(help);
        assert!(desc.is_none());
    }

    #[test]
    fn test_complex_help_output() {
        let help = "\
my-tool - A complex tool for processing data

Usage: my-tool [OPTIONS] <INPUT>

Arguments:
  <INPUT>  Input file path

Options:
  -i, --input <FILE>      Input file [required]
  -o, --output <FILE>     Output destination [default: stdout]
      --format <STR>      Output format
      --threads <INT>     Number of threads [default: 4]
      --dry-run           Perform a dry run
  -v, --verbose           Enable verbose output
  -h, --help              Print help
  -V, --version           Print version";

        let params = parser().parse(help).unwrap();
        assert_eq!(params.len(), 6); // input, output, format, threads, dry-run, verbose

        let input = params.iter().find(|p| p.name == "input").unwrap();
        assert!(input.required);
        assert_eq!(input.param_type, ParamType::String);

        let output = params.iter().find(|p| p.name == "output").unwrap();
        assert!(!output.required);
        assert_eq!(output.default_value.as_deref(), Some("stdout"));

        let threads = params.iter().find(|p| p.name == "threads").unwrap();
        assert_eq!(threads.param_type, ParamType::Integer);
        assert_eq!(threads.default_value.as_deref(), Some("4"));

        let dry_run = params.iter().find(|p| p.name == "dry-run").unwrap();
        assert_eq!(dry_run.param_type, ParamType::Boolean);
    }
}
