use std::{env::args, io::{BufRead, BufReader, BufWriter, Write}, process::{self, exit, Stdio}};

use getopts_macro::getopts_options;
use jq_bridge::{Command, Context};
use serde_json::json;

const DESC: &str = "JQ's child processes and file operation etc backend";

fn main() {
    let options = getopts_options! {
        -v, --version           "show version";
        -h, --help*             "show help message";
        .parsing_style(getopts_macro::getopts::ParsingStyle::StopAtFirstFree)
    };
    let matched = match options.parse(args().skip(1)) {
        Ok(matched) => matched,
        Err(e) => {
            eprintln!("{e}");
            exit(2)
        },
    };
    if matched.opt_present("help") {
        print!(
            "Usage: {} [Options] <jq> [args]..\n{}",
            env!("CARGO_PKG_NAME"),
            options.usage(DESC),
        );
        exit(0)
    }
    if matched.opt_present("version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        exit(0)
    }
    let Some(program) = matched.free.first() else {
        eprintln!("Expected <jq> argument!");
        exit(2)
    };
    run_jq(program, &matched.free[1..])
}

fn run_jq(program: &str, args: &[String]) -> ! {
    let ctx = &mut Context::default();

    let mut jq_coproc = process::Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .args(args)
        .spawn()
        .expect("cannot start jq coproc");

    let from = BufReader::new(jq_coproc.stdout.take().unwrap());
    let mut to = BufWriter::new(jq_coproc.stdin.take().unwrap());

    for line in from.lines() {
        let buf = line.unwrap();

        let cmd: Command = serde_json::from_str(&buf)
            .expect("invalid command");
        let value = cmd.run(ctx)
            .map(|value| json!({"ok": value}))
            .unwrap_or_else(|err| json!({"err": err.to_string()}));
        serde_json::to_writer(&mut to, &value).unwrap();
        writeln!(to).unwrap();
        to.flush().unwrap();
    }

    let code = jq_coproc.wait()
        .unwrap()
        .code()
        .unwrap_or(jq_bridge::NONE_EXIT_CODE);
    exit(code)
}
