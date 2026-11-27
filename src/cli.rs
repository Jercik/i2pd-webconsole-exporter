const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

#[derive(Debug, PartialEq, Eq)]
pub enum CliAction {
    Run,
    Print(String),
    Reject(String),
}

pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> CliAction {
    let Some(arg) = args.into_iter().next() else {
        return CliAction::Run;
    };
    match arg.as_str() {
        "-V" | "--version" => CliAction::Print(format!("{NAME} {VERSION}")),
        "-h" | "--help" => CliAction::Print(help_text()),
        _ => CliAction::Reject(format!(
            "error: unexpected argument '{arg}' found\n\nUsage: {NAME}\n\nFor more information, try '--help'."
        )),
    }
}

fn help_text() -> String {
    format!(
        "{DESCRIPTION}\n\nUsage: {NAME}\n\nOptions:\n  -h, --help     Print help\n  -V, --version  Print version"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> CliAction {
        parse_args(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn no_arguments_run_the_exporter() {
        assert_eq!(parse(&[]), CliAction::Run);
    }

    #[test]
    fn version_flags_print_name_and_version() {
        let expected = CliAction::Print("i2pd-webconsole-exporter 1.1.1".to_string());
        assert_eq!(parse(&["--version"]), expected);
        assert_eq!(parse(&["-V"]), expected);
    }

    #[test]
    fn help_flags_print_usage() {
        let expected = CliAction::Print(
            "Prometheus exporter for i2pd (via Web Console scraping)\n\nUsage: i2pd-webconsole-exporter\n\nOptions:\n  -h, --help     Print help\n  -V, --version  Print version"
                .to_string(),
        );
        assert_eq!(parse(&["--help"]), expected);
        assert_eq!(parse(&["-h"]), expected);
    }

    #[test]
    fn unknown_argument_is_rejected() {
        assert_eq!(
            parse(&["--bogus"]),
            CliAction::Reject(
                "error: unexpected argument '--bogus' found\n\nUsage: i2pd-webconsole-exporter\n\nFor more information, try '--help'."
                    .to_string()
            )
        );
    }
}
