//! Developer commands for this repository, run from the repository root:
//!
//! ```text
//! cargo xtask hil [TEST_FILTER...]
//! ```
//!
//! `hil` runs the on-device tests (ADR 0011) on a Test Board through probe-rs.
//! Those tests erase Persisted State, so it refuses to run, before building
//! anything, unless a connected probe's serial number is listed in the
//! gitignored `test-boards.txt` at the repository root. probe-rs then gets
//! that probe by serial number, so no other connected Board is touched.
//! TEST_FILTERs are passed on to probe-rs: only tests whose names contain one
//! of them run.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::{env, fs};

/// The Test Board opt-in list, relative to the repository root. Gitignored.
const TEST_BOARDS_FILE: &str = "test-boards.txt";
const TARGET: &str = "riscv32imac-unknown-none-elf";
const CHIP: &str = "esp32c6";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let result = match args.next().as_deref() {
        Some("hil") => hil(args.collect()),
        _ => Err(String::from(
            "usage: cargo xtask hil [TEST_FILTER...]\n\n  \
             hil   run the on-device tests on a Test Board (see README.md, On-device tests)",
        )),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// The repository root: xtask's manifest sits one level below it.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in a subdirectory of the repository")
        .to_path_buf()
}

fn hil(filters: Vec<String>) -> Result<ExitCode, String> {
    let root = root();
    let opted_in = read_test_boards(&root.join(TEST_BOARDS_FILE))?;
    let probe = select_test_board(&opted_in, &list_probes()?)?;
    eprintln!("Test Board: probe {}", probe.selector);

    let elf = build_tests(&root)?;
    let partition_table = root.join("partitions.csv");

    // probe-rs flashes the bootloader, the repo's partition table and the test
    // binary, then resets the chip before each test. It does not erase the
    // rest of the flash; the tests erase matter-kv themselves.
    let status = Command::new("probe-rs")
        .current_dir(&root)
        .arg("run")
        .args(["--chip", CHIP, "--probe", &probe.selector])
        .arg("--idf-partition-table")
        .arg(&partition_table)
        .arg(&elf)
        .args(&filters)
        .status()
        .map_err(|e| format!("cannot run probe-rs: {e}"))?;
    Ok(if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// The probe serial numbers listed in `test-boards.txt`: one per line, `#`
/// starts a comment.
fn read_test_boards(path: &Path) -> Result<Vec<String>, String> {
    let refusal = |why: &str| {
        format!(
            "Refusing to run the on-device tests: {why}.\n\n\
             They erase Persisted State (a commissioned Device loses its \
             Commissioning), so they only run on a Board opted in as a Test Board.\n\
             To opt a Board in, add its probe's serial number (the part after \
             `303a:1001:` in `probe-rs list`) as a line in {}.",
            path.display()
        )
    };
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(refusal(&format!("{TEST_BOARDS_FILE} does not exist")));
        }
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let serials: Vec<String> = text
        .lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|serial| !serial.is_empty())
        .map(String::from)
        .collect();
    if serials.is_empty() {
        return Err(refusal(&format!("{TEST_BOARDS_FILE} lists no Board")));
    }
    Ok(serials)
}

/// A debug probe as `probe-rs list` shows it.
#[derive(Debug, PartialEq)]
struct Probe {
    /// `VID:PID:SERIAL`, as `probe-rs --probe` takes it.
    selector: String,
    serial: Option<String>,
}

fn list_probes() -> Result<Vec<Probe>, String> {
    let output = Command::new("probe-rs")
        .arg("list")
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| {
            format!(
                "cannot run `probe-rs list` ({e}); install it with `cargo install probe-rs-tools`"
            )
        })?;
    if !output.status.success() {
        return Err(String::from("`probe-rs list` failed"));
    }
    Ok(parse_probe_list(&String::from_utf8_lossy(&output.stdout)))
}

/// Parses lines such as `[0]: ESP JTAG -- 303a:1001:58:E6:C5:F9:7D:8C (EspJtag)`.
/// The serial may contain colons: EspJtag probes use the chip's MAC address.
fn parse_probe_list(text: &str) -> Vec<Probe> {
    text.lines()
        .filter(|line| line.trim_start().starts_with('['))
        .filter_map(|line| {
            let rest = line.split_once(" -- ")?.1;
            let selector = rest.rsplit_once(" (").map_or(rest, |(s, _)| s).trim();
            let serial = selector.splitn(3, ':').nth(2).map(String::from);
            Some(Probe {
                selector: selector.to_string(),
                serial,
            })
        })
        .collect()
}

/// The one connected probe whose serial number is opted in.
fn select_test_board(opted_in: &[String], probes: &[Probe]) -> Result<Probe, String> {
    let mut boards = probes.iter().filter(|probe| {
        probe.serial.as_ref().is_some_and(|serial| {
            opted_in
                .iter()
                .any(|listed| listed.eq_ignore_ascii_case(serial))
        })
    });
    match (boards.next(), boards.next()) {
        (Some(board), None) => Ok(Probe {
            selector: board.selector.clone(),
            serial: board.serial.clone(),
        }),
        (Some(_), Some(_)) => Err(String::from(
            "Refusing to run the on-device tests: more than one opted-in Test Board is \
             connected. Connect only the one to test.",
        )),
        (None, _) => {
            let connected = if probes.is_empty() {
                String::from("No probe is connected.")
            } else {
                let list: Vec<&str> = probes.iter().map(|p| p.selector.as_str()).collect();
                format!("Connected probes: {}.", list.join(", "))
            };
            Err(format!(
                "Refusing to run the on-device tests: no connected probe is listed in \
                 {TEST_BOARDS_FILE}. {connected}\n\
                 Ran nothing. Opt a Board in only if it may lose its Persisted State \
                 (see README.md, On-device tests)."
            ))
        }
    }
}

/// Builds the Platform's on-device test binary and returns its path.
fn build_tests(root: &Path) -> Result<PathBuf, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(root)
        .args([
            "test",
            "--package",
            "platform",
            "--lib",
            "--no-run",
            "--locked",
        ])
        .args([
            "--target",
            TARGET,
            "--message-format",
            "json-render-diagnostics",
        ])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !output.status.success() {
        return Err(String::from("building the on-device tests failed"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|line| line.contains(r#""reason":"compiler-artifact""#))
        .find_map(executable)
        .map(PathBuf::from)
        .ok_or_else(|| String::from("cargo built no test binary for the Platform"))
}

/// The `"executable"` path of one cargo JSON message, if it has one.
fn executable(message: &str) -> Option<String> {
    let start = message.find(r#""executable":""#)? + r#""executable":""#.len();
    let mut path = String::new();
    let mut chars = message[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(path),
            '\\' => match chars.next()? {
                'n' => path.push('\n'),
                't' => path.push('\t'),
                escaped => path.push(escaped),
            },
            c => path.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(selector: &str, serial: &str) -> Probe {
        Probe {
            selector: selector.into(),
            serial: Some(serial.into()),
        }
    }

    #[test]
    fn parses_probe_rs_list() {
        let text = "The following debug probes were found:\n\
                    [0]: ESP JTAG -- 303a:1001:58:E6:C5:F9:7D:8C (EspJtag)\n\
                    [1]: STLink V3 -- 0483:374e:002A00253137510739383538 (ST-LINK)\n";
        assert_eq!(
            parse_probe_list(text),
            [
                probe("303a:1001:58:E6:C5:F9:7D:8C", "58:E6:C5:F9:7D:8C"),
                probe(
                    "0483:374e:002A00253137510739383538",
                    "002A00253137510739383538"
                ),
            ]
        );
        assert_eq!(parse_probe_list("No debug probes were found.\n"), []);
    }

    #[test]
    fn selects_only_an_opted_in_probe() {
        let board = probe("303a:1001:58:E6:C5:F9:7D:8C", "58:E6:C5:F9:7D:8C");
        let other = probe("303a:1001:AA:BB:CC:DD:EE:FF", "AA:BB:CC:DD:EE:FF");
        let opted_in = [String::from("58:e6:c5:f9:7d:8c")];

        let probes = [other, board];
        assert_eq!(
            select_test_board(&opted_in, &probes).unwrap().selector,
            "303a:1001:58:E6:C5:F9:7D:8C"
        );
        assert!(select_test_board(&opted_in, &probes[..1]).is_err());
        assert!(select_test_board(&opted_in, &[]).is_err());
        assert!(select_test_board(&[], &probes).is_err());
    }

    #[test]
    fn reads_the_executable_path() {
        let message = r#"{"reason":"compiler-artifact","executable":"C:\\A\\target\\deps\\platform-1a2b","fresh":true}"#;
        assert_eq!(
            executable(message).as_deref(),
            Some(r"C:\A\target\deps\platform-1a2b")
        );
        assert_eq!(executable(r#"{"executable":null}"#), None);
    }
}
