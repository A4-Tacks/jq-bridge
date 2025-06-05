JQ's child processes and file operation etc backend

Rough Feature List:

- read, write and append to file
- print to stdout
- run subprocess
- generate random number
- get and set env

# Example
```bash
$ cargo run jq --unbuffered -ncf example.jq
   Compiling jq-bridge v0.1.0 (/path/to/jq-bridge)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.77s
     Running `target/debug/jq-bridge jq --unbuffered -ncf example.jq`
Hello, World!
file:
[package]
name = "jq-bridge"
version = "0.1.0"
edition = "2024"

[dependencies]
getopts-macro = "0.1.4"
rand = "0.9.1"
serde = { version = "1.0.219", features = ["derive"] }
serde_json = "1.0.140"
thiserror = "2.0.12"
time = "0.3.41"

random: 0.44349047669262176
Run ls -a
status: 0, output:
.
..
Cargo.lock
Cargo.toml
example.jq
.git
.gitignore
README.md
src
target
```
