//! The actions Local CI does itself, written for it: `actions/checkout`,
//! `setup-node`, `setup-python`, `setup-go`, `setup-java`, `cache`, and
//! `upload-artifact` / `download-artifact`. They read their inputs from
//! `INPUT_*` variables, as real actions do; downloaded tools are kept in a
//! volume, so the next run has them already.

/// Where downloaded tools are kept (a volume shared by every run).
pub const TOOL_CACHE: &str = "/opt/hostedtoolcache";
pub const TOOL_CACHE_VOLUME: &str = "ferrodock-ci-toolcache";
/// Where `actions/cache` keeps what it saves (a volume too).
pub const CACHE_DIR: &str = "/opt/ferrodock-cache";
pub const CACHE_VOLUME: &str = "ferrodock-ci-cache";
/// Where a run's artifacts are (a volume of the run).
pub const ARTIFACTS_DIR: &str = "/opt/ferrodock-artifacts";

/// What a `uses:` step does here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Builtin {
    /// Copies the project into the workspace (done by the runner).
    Checkout,
    /// A shell script, with a script to run when the job ends (for
    /// `actions/cache`, saving).
    Script {
        main: &'static str,
        post: Option<&'static str>,
    },
    /// An artifact is saved under this input's value.
    UploadArtifact,
    /// Not done here yet; the step is skipped with a warning.
    Unsupported,
}

/// What `uses:` (e.g. `actions/setup-node@v4`) means here.
pub fn builtin(uses: &str) -> Builtin {
    let name = uses.split('@').next().unwrap_or(uses).to_lowercase();
    match name.as_str() {
        "actions/checkout" => Builtin::Checkout,
        "actions/setup-node" => Builtin::Script {
            main: SETUP_NODE,
            post: None,
        },
        "actions/setup-python" => Builtin::Script {
            main: SETUP_PYTHON,
            post: None,
        },
        "actions/setup-go" => Builtin::Script {
            main: SETUP_GO,
            post: None,
        },
        "actions/setup-java" => Builtin::Script {
            main: SETUP_JAVA,
            post: None,
        },
        "actions/cache" => Builtin::Script {
            main: CACHE_RESTORE,
            post: Some(CACHE_SAVE),
        },
        "actions/cache/restore" => Builtin::Script {
            main: CACHE_RESTORE,
            post: None,
        },
        "actions/cache/save" => Builtin::Script {
            main: CACHE_SAVE,
            post: None,
        },
        "actions/upload-artifact" => Builtin::UploadArtifact,
        "actions/download-artifact" => Builtin::Script {
            main: DOWNLOAD_ARTIFACT,
            post: None,
        },
        // Rust's toolchain action is common enough to matter.
        "dtolnay/rust-toolchain" | "actions-rust-lang/setup-rust-toolchain" => Builtin::Script {
            main: SETUP_RUST,
            post: None,
        },
        _ => Builtin::Unsupported,
    }
}

/// `INPUT_NODE_VERSION` for the input `node-version`, as GitHub names them.
pub fn input_env(name: &str) -> String {
    format!("INPUT_{}", name.replace(' ', "_").to_uppercase())
}

/// The machine, as tool downloads name it.
const ARCH: &str = r#"arch=$(uname -m | sed 's/x86_64/x64/;s/aarch64/arm64/')"#;

const SETUP_NODE: &str = r#"
v="${INPUT_NODE_VERSION:-}"
if [ -z "$v" ] && [ -n "${INPUT_NODE_VERSION_FILE:-}" ]; then v=$(tr -d ' \r\n' < "$INPUT_NODE_VERSION_FILE"); fi
if [ -z "$v" ]; then echo "Using Node $(node --version) from the runner"; exit 0; fi
ARCH_LINE
full=$(curl -fsSL https://nodejs.org/dist/index.json | WANT="$v" python3 -c '
import json, os, sys
want = os.environ["WANT"].strip().lstrip("v")
for r in json.load(sys.stdin):
    v = r["version"].lstrip("v")
    if want in ("lts/*", "lts") and r["lts"]: print(v); break
    if want in ("latest", "current", "node"): print(v); break
    w = want.replace(".x", "")
    if v == w or v.startswith(w + "."): print(v); break
')
[ -n "$full" ] || { echo "::error::There's no Node $v"; exit 1; }
dir="$RUNNER_TOOL_CACHE/node/$full/$arch"
if [ ! -x "$dir/bin/node" ]; then
  echo "Downloading Node $full"
  mkdir -p "$dir"
  curl -fsSL "https://nodejs.org/dist/v$full/node-v$full-linux-$arch.tar.xz" | tar -xJ -C "$dir" --strip-components=1
fi
echo "$dir/bin" >> "$GITHUB_PATH"
echo "Node $full"
"#;

const SETUP_PYTHON: &str = r#"
v="${INPUT_PYTHON_VERSION:-}"
if [ -z "$v" ] && [ -f "${INPUT_PYTHON_VERSION_FILE:-.python-version}" ]; then v=$(head -1 "${INPUT_PYTHON_VERSION_FILE:-.python-version}" | tr -d ' \r'); fi
if [ -z "$v" ]; then echo "Using $(python3 --version) from the runner"; exit 0; fi
ARCH_LINE
os=$(. /etc/os-release; echo "$VERSION_ID")
url=$(curl -fsSL https://raw.githubusercontent.com/actions/python-versions/main/versions-manifest.json | WANT="$v" OS="$os" ARCH="$arch" python3 -c '
import json, os, sys
want = os.environ["WANT"].replace(".x", "")
for r in json.load(sys.stdin):
    v = r["version"]
    if not (v == want or v.startswith(want + ".")) or (not r["stable"] and v != want): continue
    for f in r["files"]:
        if f["platform"] == "linux" and f["platform_version"] == os.environ["OS"] and f["arch"] == os.environ["ARCH"]:
            print(v, f["download_url"]); sys.exit()
')
[ -n "$url" ] || { echo "::error::There's no Python $v for Ubuntu $os ($arch)"; exit 1; }
full=${url%% *}
dir="$RUNNER_TOOL_CACHE/Python/$full/$arch"
if [ ! -x "$dir/bin/python3" ]; then
  echo "Downloading Python $full"
  tmp=$(mktemp -d)
  curl -fsSL "${url#* }" | tar -xz -C "$tmp"
  (cd "$tmp" && sudo -E env AGENT_TOOLSDIRECTORY="$RUNNER_TOOL_CACHE" RUNNER_TOOL_CACHE="$RUNNER_TOOL_CACHE" bash ./setup.sh >/dev/null)
  sudo chown -R "$(id -u):$(id -g)" "$RUNNER_TOOL_CACHE/Python" 2>/dev/null || true
fi
echo "$dir/bin" >> "$GITHUB_PATH"
echo "pythonLocation=$dir" >> "$GITHUB_ENV"
echo "Python $full"
"#;

const SETUP_GO: &str = r#"
v="${INPUT_GO_VERSION:-}"
if [ -z "$v" ] && [ -f "${INPUT_GO_VERSION_FILE:-}" ]; then v=$(sed -n 's/^go \([0-9.]*\).*/\1/p' "$INPUT_GO_VERSION_FILE" | head -1); fi
[ -n "$v" ] || v=stable
ARCH_LINE
goarch=$(echo "$arch" | sed 's/x64/amd64/')
full=$(curl -fsSL 'https://go.dev/dl/?mode=json&include=all' | WANT="$v" python3 -c '
import json, os, sys
want = os.environ["WANT"].replace(".x", "").lstrip("v")
for r in json.load(sys.stdin):
    v = r["version"][2:]
    if not r["stable"]: continue
    if want in ("stable", "latest") or v == want or v.startswith(want + "."): print(v); break
')
[ -n "$full" ] || { echo "::error::There's no Go $v"; exit 1; }
dir="$RUNNER_TOOL_CACHE/go/$full/$arch"
if [ ! -x "$dir/bin/go" ]; then
  echo "Downloading Go $full"
  mkdir -p "$dir"
  curl -fsSL "https://go.dev/dl/go$full.linux-$goarch.tar.gz" | tar -xz -C "$dir" --strip-components=1
fi
echo "$dir/bin" >> "$GITHUB_PATH"
echo "$HOME/go/bin" >> "$GITHUB_PATH"
echo "Go $full"
"#;

const SETUP_JAVA: &str = r#"
v="${INPUT_JAVA_VERSION:-}"
[ -n "$v" ] || { echo "::error::setup-java needs a java-version"; exit 1; }
v=${v%%.*}
case "${INPUT_DISTRIBUTION:-temurin}" in temurin|adopt*) ;; *) echo "::notice::Using Temurin for ${INPUT_DISTRIBUTION}";; esac
ARCH_LINE
jarch=$(echo "$arch" | sed 's/arm64/aarch64/')
dir="$RUNNER_TOOL_CACHE/Java_Temurin-Hotspot_jdk/$v/$arch"
if [ ! -x "$dir/bin/java" ]; then
  echo "Downloading Temurin JDK $v"
  mkdir -p "$dir"
  curl -fsSL "https://api.adoptium.net/v3/binary/latest/$v/ga/linux/$jarch/jdk/hotspot/normal/eclipse" | tar -xz -C "$dir" --strip-components=1
fi
echo "$dir/bin" >> "$GITHUB_PATH"
echo "JAVA_HOME=$dir" >> "$GITHUB_ENV"
"$dir/bin/java" -version 2>&1 | head -1
"#;

const SETUP_RUST: &str = r#"
t="${INPUT_TOOLCHAIN:-stable}"
if ! command -v rustup >/dev/null 2>&1 && [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  echo "Installing rustup"
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none >/dev/null
fi
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install "$t" --profile minimal ${INPUT_COMPONENTS:+--component $(echo "$INPUT_COMPONENTS" | tr ',' ' ')} ${INPUT_TARGETS:+--target $(echo "$INPUT_TARGETS" | tr ',' ' ')}
rustup default "$t"
echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"
rustc --version
"#;

/// Which paths `actions/cache` keeps: one per line, `~` for home.
const CACHE_PATHS: &str = r#"
paths=$(printf '%s\n' "$INPUT_PATH" | sed "s|^~|$HOME|" | grep -v '^\s*$')
dir="$CACHE_DIR/$(printf '%s' "$INPUT_KEY" | sha256sum | cut -c1-32)"
"#;

const CACHE_RESTORE: &str = r#"
CACHE_PATHS_LINES
hit=""
if [ -f "$dir/data.tar" ]; then hit="$dir"; echo "cache-hit=true" >> "$GITHUB_OUTPUT"
else
  echo "cache-hit=false" >> "$GITHUB_OUTPUT"
  for prefix in $(printf '%s\n' "${INPUT_RESTORE_KEYS:-}" | grep -v '^\s*$'); do
    hit=$(grep -lx -- "$prefix.*" "$CACHE_DIR"/*/key 2>/dev/null | head -1 | xargs -r dirname)
    [ -n "$hit" ] && break
  done
fi
if [ -n "$hit" ]; then
  tar -xf "$hit/data.tar" -C / && echo "Restored the cache $(cat "$hit/key")"
  echo "cache-matched-key=$(cat "$hit/key")" >> "$GITHUB_OUTPUT"
else
  echo "No cache for $INPUT_KEY"
fi
"#;

const CACHE_SAVE: &str = r#"
CACHE_PATHS_LINES
if [ -f "$dir/data.tar" ]; then echo "The cache $INPUT_KEY is saved already"; exit 0; fi
files=""
for p in $paths; do for f in $p; do [ -e "$f" ] && files="$files $(realpath "$f")"; done; done
[ -n "$files" ] || { echo "Nothing to cache for $INPUT_KEY"; exit 0; }
mkdir -p "$dir"
tar -cf "$dir/data.tar" -C / $(for f in $files; do printf '%s ' "${f#/}"; done)
printf '%s' "$INPUT_KEY" > "$dir/key"
echo "Saved the cache $INPUT_KEY ($(du -sh "$dir/data.tar" | cut -f1))"
"#;

/// Puts the files of `path` in the run's artifacts, under `name`.
pub const UPLOAD_ARTIFACT: &str = r#"
name="${INPUT_NAME:-artifact}"
dest="$ARTIFACTS_DIR/$name"
files=""
for p in $(printf '%s\n' "$INPUT_PATH" | grep -v '^\s*$'); do for f in $p; do [ -e "$f" ] && files="$files $f"; done; done
if [ -z "$files" ]; then
  case "${INPUT_IF_NO_FILES_FOUND:-warn}" in
    error) echo "::error::No files were found for $name"; exit 1;;
    ignore) exit 0;;
    *) echo "::warning::No files were found for $name"; exit 0;;
  esac
fi
mkdir -p "$dest"
tar -cf - $files | tar -xf - -C "$dest"
echo "Saved the artifact $name ($(du -sh "$dest" | cut -f1))"
"#;

const DOWNLOAD_ARTIFACT: &str = r#"
target="${INPUT_PATH:-.}"
mkdir -p "$target"
if [ -n "${INPUT_NAME:-}" ]; then
  [ -d "$ARTIFACTS_DIR/$INPUT_NAME" ] || { echo "::error::There's no artifact $INPUT_NAME"; exit 1; }
  cp -a "$ARTIFACTS_DIR/$INPUT_NAME/." "$target/"
  echo "Downloaded $INPUT_NAME to $target"
else
  for d in "$ARTIFACTS_DIR"/*/; do [ -d "$d" ] && cp -a "$d" "$target/" && echo "Downloaded $(basename "$d")"; done
fi
"#;

/// A built-in script, ready to run with `bash -e`.
pub fn script(body: &str) -> String {
    let body = body
        .replace("ARCH_LINE", ARCH)
        .replace("CACHE_PATHS_LINES", CACHE_PATHS);
    format!(
        "set -eo pipefail\nRUNNER_TOOL_CACHE=\"${{RUNNER_TOOL_CACHE:-{TOOL_CACHE}}}\"\nCACHE_DIR={CACHE_DIR}\nARTIFACTS_DIR={ARTIFACTS_DIR}\n{body}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_common_actions() {
        assert_eq!(builtin("actions/checkout@v4"), Builtin::Checkout);
        assert!(matches!(
            builtin("actions/setup-node@v4"),
            Builtin::Script { post: None, .. }
        ));
        assert!(matches!(
            builtin("actions/cache@v4"),
            Builtin::Script { post: Some(_), .. }
        ));
        assert_eq!(
            builtin("actions/upload-artifact@v4"),
            Builtin::UploadArtifact
        );
        assert_eq!(builtin("someone/else@v1"), Builtin::Unsupported);
        assert_eq!(input_env("node-version"), "INPUT_NODE-VERSION");
    }

    #[test]
    fn scripts_are_complete() {
        for body in [
            SETUP_NODE,
            SETUP_PYTHON,
            SETUP_GO,
            SETUP_JAVA,
            SETUP_RUST,
            CACHE_RESTORE,
            CACHE_SAVE,
            UPLOAD_ARTIFACT,
            DOWNLOAD_ARTIFACT,
        ] {
            let s = script(body);
            assert!(!s.contains("ARCH_LINE") && !s.contains("CACHE_PATHS_LINES"));
            assert!(s.starts_with("set -eo pipefail"));
        }
    }
}
