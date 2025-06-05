def rt_unpack:
  if has("pipe") then
    halt_error
  else
    .send
  end
  ;
def pipe(f):
  if has("pipe") then
    .pipe|f
  end
  ;
def unwrap:
  input | if has("err") then
    "error: \(.err)\n"|halt_error(3)
  end | .ok;
def read($value):
  {send: {read: $value}},
  {pipe: .}
  ;
def println($value):
  {send: {println: $value}},
  (unwrap|empty),
  {pipe: .}
  ;
def random_float:
  {send: "random_float"},
  {pipe: .}
  ;
def popen(args):
  {send: {popen: [args]|[first, .[1:]]}},
  {pipe: .}
  ;
println("Hello, World!") |
pipe(read("Cargo.toml")) |
pipe(println("file:\n\(unwrap)")) |
pipe(random_float) |
pipe(println("random: \(unwrap)")) |
pipe(println("Run ls -a")) |
pipe(popen("ls", "-a")) |
pipe(unwrap|println("status: \(.status), output:\n\(.stdout)")) |
pipe(empty) |
rt_unpack
