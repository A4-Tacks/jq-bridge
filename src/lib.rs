use std::{
    collections::HashMap,
    convert::identity,
    env,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, stdin, stdout, Write},
    iter,
    path::Path,
    process::{self, exit, Child, Stdio},
    thread::spawn,
    time::SystemTime,
};

use rand::{rngs::ThreadRng, Rng};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value::{self, Null}};
use thiserror::Error;
use time::UtcDateTime;

pub trait IsTrue {
    fn is_true(&self) -> bool;
}
impl IsTrue for Option<bool> {
    fn is_true(&self) -> bool {
        self.is_some_and(identity)
    }
}
pub trait IsFalse {
    fn is_false(&self) -> bool;
}
impl IsFalse for Option<bool> {
    fn is_false(&self) -> bool {
        self.is_none_or(|b| !b)
    }
}
pub trait MapResult: Iterator<Item = Result<Self::Ok, Self::Err>> + Sized {
    type Ok;
    type Err;

    fn map_ok<F, U>(self, mut f: F) -> iter::Map<
        Self,
        impl FnMut(Self::Item) -> Result<U, Self::Err>,
    >
    where F: FnMut(Self::Ok) -> U,
    {
        self.map(move |value| value.map(&mut f))
    }

    fn map_and<F, U, UE>(self, mut f: F) -> iter::Map<
        Self,
        impl FnMut(Self::Item) -> Result<U, UE>,
    >
    where F: FnMut(Self::Ok) -> Result<U, UE>,
          Self::Err: Into<UE>,
    {
        self.map(move |value| {
            value.map_err(Into::into)
                .and_then(&mut f)
        })
    }

    fn map_err<F, UE>(self, mut f: F) -> iter::Map<
        Self,
        impl FnMut(Self::Item) -> Result<Self::Ok, UE>,
    >
    where F: FnMut(Self::Err) -> UE,
    {
        self.map(move |value| value.map_err(&mut f))
    }
}
impl<T, E, I: Iterator<Item = Result<T, E>>> MapResult for I {
    type Ok = T;
    type Err = E;
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CommandBuilder {
    args: Option<Vec<String>>,
    env_clear: Option<bool>,
    envs: Option<HashMap<String, String>>,
    remove_envs: Option<Vec<String>>,
    current_dir: Option<String>,
    stdin: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
    stdout_append: Option<bool>,
    stderr_append: Option<bool>,
}
impl CommandBuilder {
    pub fn apply<F>(
        &self,
        mut command: process::Command,
        f: F,
    ) -> Result<Child, Error>
    where F: FnOnce(process::Command) -> Result<Child, Error>,
    {
        let CommandBuilder {
            args,
            env_clear,
            envs,
            remove_envs,
            current_dir,
            stdin,
            stdout,
            stderr,
            stdout_append,
            stderr_append,
        } = self;

        if let Some(args) = args {
            command.args(args);
        }

        command.envs(envs.iter().flatten());

        if env_clear.is_true() {
            command.env_clear();
        }

        for name in remove_envs.iter().flatten() {
            command.env_remove(name);
        }

        if let Some(current_dir) = current_dir {
            command.current_dir(current_dir);
        }

        let stdin = stdin.as_ref()
            .map(File::open)
            .transpose()?;
        let stdout = stdout.as_ref()
            .map(|path| OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(stdout_append.is_false())
                .append(stdout_append.is_true())
                .open(path))
            .transpose()?;
        let stderr = stderr.as_ref()
            .map(|path| OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(stderr_append.is_false())
                .append(stderr_append.is_true())
                .open(path))
            .transpose()?;

        let mut child = f(command)?;

        let mut child_stdin = child.stdin.take().unwrap();
        let mut child_stdout = child.stdout.take().unwrap();
        let mut child_stderr = child.stderr.take().unwrap();

        let jobs = [
            spawn(move || stdin.map(|mut in_file| {
                io::copy(&mut in_file, &mut child_stdin)
            }).transpose()),
            spawn(move || stdout.map(|mut out_file| {
                io::copy(&mut child_stdout, &mut out_file)
            }).transpose()),
            spawn(move || stderr.map(|mut err_file| {
                io::copy(&mut child_stderr, &mut err_file)
            }).transpose()),
        ];

        for job in jobs {
            job.join().unwrap()?;
        }

        Ok(child)
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Deserialize, Serialize)]
pub enum Command {
    read(String),
    write { path: String, text: String, must_new: Option<bool> },
    append { path: String, text: String, must_exist: Option<bool> },
    read_dir(String),
    read_link(String),
    metadata(String),
    metadata_extra(String),
    exists(String),
    is_symlink(String),
    is_dir(String),
    is_file(String),
    print(Value),
    println(Value),
    pretty(Value),
    pretty_pipe(Value),
    stdin,
    stdin_line,
    current_dir,
    temp_dir,
    get_env(String),
    set_env(String, String),
    remove_env(String),
    system(String, Vec<String>),
    popen(String, Vec<String>),
    command(String, CommandBuilder),
    wait_id { id: u32, output: Option<bool> },
    kill_id { id: u32 },
    process_id,
    random,
    random_float,
    exit(i32),
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    IoError(#[from] io::Error),
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("invalid string: {0:?}")]
    InvalidString(String),
    #[error("invalid processor id: {0}")]
    InvalidProcessorId(u32),
}

pub const NONE_EXIT_CODE: i32 = 250;

fn path_it(path: impl AsRef<Path>) -> Result<Value, Error> {
    let path = path.as_ref();
    path.to_str()
        .map(Into::into)
        .ok_or_else(|| Error::InvalidString(path.to_string_lossy().into()))
}

fn oss_it(path: impl AsRef<OsStr>) -> Result<Value, Error> {
    let s = path.as_ref();
    s.to_str()
        .map(Into::into)
        .ok_or_else(|| Error::InvalidString(s.to_string_lossy().into()))
}

fn time_it(time: SystemTime) -> String {
    UtcDateTime::from(time).to_string()
}

impl Command {
    pub fn run(&self, ctx: &mut Context) -> Result<Value, Error> {
        Ok(match self {
            Command::read(path) => {
                fs::read_to_string(path)?.into()
            },
            Command::write { path, text, must_new } => {
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .create_new(must_new.is_true())
                    .open(path)?
                    .write_all(text.as_bytes())?;
                Null
            },
            Command::append { path, text, must_exist } => {
                OpenOptions::new()
                    .append(must_exist.is_false())
                    .create(true)
                    .open(path)?
                    .write_all(text.as_bytes())?;
                Null
            },
            Command::read_dir(path) => {
                let paths = fs::read_dir(path)?
                    .map_and(|dir| path_it(dir.path()))
                    .collect::<Result<Vec<Value>, _>>()?;
                paths.into()
            },
            Command::read_link(link) => {
                path_it(fs::read_link(link)?)?
            },
            Command::metadata(path) => {
                let metadata = fs::metadata(path)?;
                json!({
                    "readonly": metadata.permissions().readonly(),
                    "is_file": metadata.is_file(),
                    "id_dir": metadata.is_dir(),
                    "len": metadata.len(),
                })
            },
            Command::metadata_extra(path) => {
                let metadata = fs::metadata(path)?;
                json!({
                    "readonly": metadata.permissions().readonly(),
                    "is_file": metadata.is_file(),
                    "id_dir": metadata.is_dir(),
                    "len": metadata.len(),
                    "accessed": time_it(metadata.accessed()?),
                    "modified": time_it(metadata.modified()?),
                    "created": time_it(metadata.created()?),
                })
            },
            Command::exists(path) => {
                fs::exists(path)?.into()
            },
            Command::is_symlink(path) => {
                fs::symlink_metadata(path)?.is_symlink().into()
            },
            Command::is_dir(path) => {
                fs::metadata(path)?.is_dir().into()
            },
            Command::is_file(path) => {
                fs::metadata(path)?.is_file().into()
            },
            Command::print(value) => {
                if let Some(s) = value.as_str() {
                    stdout().write_all(s.as_bytes())?;
                } else {
                    serde_json::to_writer(stdout().lock(), value)?;
                }
                Null
            },
            Command::println(value) => {
                let mut writer = stdout().lock();
                if let Some(s) = value.as_str() {
                    stdout().write_all(s.as_bytes())?;
                } else {
                    serde_json::to_writer(&mut writer, value)?;
                }
                writer.write_all(b"\n")?;
                Null
            },
            Command::pretty(value) => {
                let mut writer = stdout().lock();
                serde_json::to_writer_pretty(&mut writer, value)?;
                writer.write_all(b"\n")?;
                Null
            },
            Command::pretty_pipe(value) => {
                let mut writer = stdout().lock();
                serde_json::to_writer(&mut writer, value)?;
                writer.write_all(b"\n")?;
                Null
            },
            Command::stdin => {
                io::read_to_string(stdin().lock())?.into()
            },
            Command::stdin_line => {
                let mut buf = String::new();
                stdin().read_line(&mut buf)?;
                buf.into()
            },
            Command::current_dir => {
                path_it(env::current_dir()?)?
            },
            Command::temp_dir => {
                path_it(env::temp_dir())?
            },
            Command::get_env(name) => {
                env::var_os(name)
                    .map(oss_it)
                    .transpose()?
                    .unwrap_or(Value::Null)
            },
            Command::set_env(name, value) => {
                unsafe { env::set_var(name, value) }
                Null
            },
            Command::remove_env(name) => {
                unsafe { env::remove_var(name) }
                Null
            },
            Command::system(prog, args) => {
                process::Command::new(prog)
                    .args(args)
                    .status()?
                    .code()
                    .unwrap_or(NONE_EXIT_CODE)
                    .into()
            },
            Command::popen(prog, args) => {
                let output = process::Command::new(prog)
                    .args(args)
                    .stderr(Stdio::inherit())
                    .output()?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                json!({
                    "stdout": stdout,
                    "status": output.status.code().unwrap_or(NONE_EXIT_CODE),
                })
            },
            Command::command(prog, command_builder) => {
                let command = process::Command::new(prog);
                let child = command_builder.apply(command, |mut cmd| {
                    Ok(cmd.spawn()?)
                })?;
                let output = child.wait_with_output()?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "status": output.status.code().unwrap_or(NONE_EXIT_CODE),
                })
            },
            Command::wait_id { id, output } => {
                if output.is_true() {
                    let output = ctx.child(*id)?.wait_with_output()?;
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    json!({
                        "stdout": stdout,
                        "stderr": stderr,
                        "status": output.status.code().unwrap_or(NONE_EXIT_CODE),
                    })
                } else {
                    ctx.child(*id)?.wait()?.code().unwrap_or(NONE_EXIT_CODE).into()
                }
            },
            Command::kill_id { id } => {
                ctx.child(*id)?.kill()?;
                Null
            },
            Command::process_id => process::id().into(),
            Command::random => {
                ctx.thread_rng.random::<u64>().into()
            },
            Command::random_float => {
                ctx.thread_rng.random::<f64>().into()
            },
            Command::exit(code) => exit(*code),
        })
    }
}

#[derive(Debug, Default)]
pub struct Context {
    sub_processors: HashMap<u32, Child>,
    thread_rng: ThreadRng,
}

impl Context {
    pub fn child(&mut self, id: u32) -> Result<Child, Error> {
        self.sub_processors.remove(&id).ok_or(Error::InvalidProcessorId(id))
    }
}
