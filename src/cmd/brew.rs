use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use walkdir::WalkDir;

use crate::config::TomlConfig;
use crate::skill::parser::parse_skill;

const GITHUB_REPO: &str = "mlnja/tama";

// Always bundled in every image — needed for HTTPS, timezones, shell tools
const ALWAYS_APT: &[&str] = &["ca-certificates", "tzdata", "bash", "curl"];

pub fn run() -> Result<()> {
    if !std::path::Path::new("tama.toml").exists() {
        bail!("no tama.toml found — run this command inside a tama project");
    }
    let config = TomlConfig::load()?;

    // ── Collect deps from skills/ ─────────────────────────────────────────────
    let mut uv_deps: BTreeSet<String> = BTreeSet::new();
    let mut apt_deps: BTreeSet<String> = BTreeSet::new();
    let mut bins: BTreeSet<String> = BTreeSet::new();
    let mut has_python = false;

    for entry in WalkDir::new("skills").into_iter().filter_map(|e| e.ok()) {
        // Detect Python files
        if entry.path().extension().and_then(|e| e.to_str()) == Some("py") {
            has_python = true;
        }
        if entry.file_name() != "SKILL.md" {
            continue;
        }
        match parse_skill(entry.path()) {
            Ok(skill) => {
                if let Some(deps) = skill.tama.depends {
                    if !deps.uv.is_empty() {
                        has_python = true;
                    }
                    uv_deps.extend(deps.uv);
                    apt_deps.extend(deps.apt);
                    bins.extend(deps.bins);
                }
            }
            Err(e) => eprintln!("warning: skipping {}: {}", entry.path().display(), e),
        }
    }

    // ── Inject always-bundled packages ────────────────────────────────────────
    for pkg in ALWAYS_APT {
        apt_deps.insert(pkg.to_string());
    }

    // ── Get tamad linux/amd64 binary ──────────────────────────────────────────
    let tamad_path = get_tamad_linux()?;

    // ── Discover apt bundle paths via temporary container ─────────────────────
    eprintln!("discovering apt file bundle (this takes ~30-60s)...");
    let bundle_paths = discover_apt_bundle(&apt_deps, &bins)?;
    eprintln!("discovered {} files to bundle", bundle_paths.len());

    // ── Generate Dockerfile ───────────────────────────────────────────────────
    let dockerfile = generate_dockerfile(
        &uv_deps,
        &bundle_paths,
        has_python,
        &config.project.entrypoint,
    );

    // ── Build context in a tmpdir ─────────────────────────────────────────────
    let ctx = tempfile::TempDir::new().context("failed to create temp dir")?;
    std::fs::copy(&tamad_path, ctx.path().join("tamad")).context("failed to copy tamad binary")?;

    if Path::new("agents").exists() {
        copy_dir("agents", ctx.path().join("agents"))?;
    }
    if Path::new("skills").exists() {
        copy_dir("skills", ctx.path().join("skills"))?;
    }
    std::fs::copy("tama.toml", ctx.path().join("tama.toml")).context("failed to copy tama.toml")?;

    // ── docker build via stdin ────────────────────────────────────────────────
    let image_name = &config.project.name;
    eprintln!("building image '{image_name}'...");

    let mut child = Command::new("docker")
        .args([
            "build",
            "--platform",
            "linux/amd64",
            "-t",
            image_name,
            "-f",
            "-",
            ".",
        ])
        .current_dir(ctx.path())
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to run docker build — is Docker running?")?;

    child
        .stdin
        .take()
        .unwrap()
        .write_all(dockerfile.as_bytes())
        .context("failed to write Dockerfile to docker stdin")?;

    let status = child.wait()?;
    if !status.success() {
        bail!("docker build failed");
    }

    println!("image '{image_name}' ready");
    println!();
    println!("run it:");
    println!("  docker run --rm \\");
    println!("    -e ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY \\");
    println!("    {image_name} \"your task here\"");

    Ok(())
}

// ── Binary download ───────────────────────────────────────────────────────────

fn get_tamad_linux() -> Result<PathBuf> {
    let version = env!("CARGO_PKG_VERSION");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let cache = PathBuf::from(home).join(".tama").join("cache");
    std::fs::create_dir_all(&cache).context("failed to create ~/.tama/cache")?;

    let cached = cache.join(format!("tamad-linux-amd64-{version}"));
    if cached.exists() {
        eprintln!("using cached tamad {version} (linux/amd64)");
        return Ok(cached);
    }

    let url =
        format!("https://github.com/{GITHUB_REPO}/releases/download/v{version}/tamad-linux-amd64");
    eprintln!("downloading tamad {version} for linux/amd64...");

    let bytes = reqwest::blocking::get(&url)
        .with_context(|| format!("failed to download tamad from {url}"))?
        .error_for_status()
        .with_context(|| format!("server error downloading tamad from {url}"))?
        .bytes()
        .context("failed to read tamad binary bytes")?;

    // Supply-chain verification: compare SHA256 of downloaded binary against the
    // value baked into this tama binary at compile time.  When building locally
    // (without the env var set) the constant is None and verification is skipped.
    const EXPECTED_SHA256: Option<&str> = option_env!("TAMAD_LINUX_AMD64_SHA256");
    if let Some(expected) = EXPECTED_SHA256 {
        let actual = hex::encode(Sha256::digest(&bytes));
        if actual != expected {
            bail!(
                "tamad SHA256 mismatch — supply-chain check failed\n  expected: {expected}\n  actual:   {actual}"
            );
        }
        eprintln!("tamad SHA256 verified ✓");
    }

    std::fs::write(&cached, &bytes).context("failed to write tamad to cache")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cached, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(cached)
}

// ── apt file discovery ────────────────────────────────────────────────────────

fn discover_apt_bundle(
    apt_pkgs: &BTreeSet<String>,
    bins: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let pkgs_str = apt_pkgs
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    let dpkg_query = apt_pkgs
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    // Build ldd section: explicit bins from skills, or auto-discover from /usr/bin /bin
    let ldd_cmds = if bins.is_empty() {
        // ldd every executable newly installed (best-effort)
        "find /usr/bin /usr/sbin /bin /sbin -maxdepth 1 -type f -executable 2>/dev/null \
         | xargs -I{} sh -c 'ldd {} 2>/dev/null | awk \"/=>/ { print \\$3 }\" | grep -E \"^/\"' \
         | sort -u"
            .to_string()
    } else {
        bins.iter()
            .map(|b| {
                format!(
                    "ldd $(which {b} 2>/dev/null || echo /usr/bin/{b}) 2>/dev/null \
                     | awk '/=>/ {{ print $3 }}' | grep -E '^/'"
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let script = format!(
        r#"
set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq 2>/dev/null
apt-get install -y --no-install-recommends {pkgs_str} 2>/dev/null
echo '=DPKG='
dpkg -L {dpkg_query} 2>/dev/null | sort -u
echo '=LDD='
{ldd_cmds}
"#
    );

    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--platform",
            "linux/amd64",
            "debian:bookworm-slim",
            "bash",
            "-c",
            &script,
        ])
        .output()
        .context("failed to run apt discovery container")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("apt discovery container failed:\n{stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut paths: BTreeSet<String> = BTreeSet::new();
    let mut section = "";

    for line in stdout.lines() {
        match line {
            "=DPKG=" => {
                section = "dpkg";
                continue;
            }
            "=LDD=" => {
                section = "ldd";
                continue;
            }
            _ => {}
        }
        if line.starts_with('/') && !line.ends_with("/.") && line != "/" {
            // Skip pure-directory entries from dpkg (they don't need cp --parents)
            // Keep everything else — files and symlinks
            paths.insert(line.to_string());
        }
        let _ = section; // used for future per-section filtering if needed
    }

    Ok(paths.into_iter().collect())
}

// ── Dockerfile generation ─────────────────────────────────────────────────────

fn generate_dockerfile(
    uv_deps: &BTreeSet<String>,
    bundle_paths: &[String],
    has_python: bool,
    entrypoint: &str,
) -> String {
    let mut out = String::new();

    // Stage 1: uv Python packages
    if has_python && !uv_deps.is_empty() {
        out.push_str("FROM ghcr.io/astral-sh/uv:debian AS uv-builder\n");
        let pkgs = uv_deps
            .iter()
            .map(|p| format!("    {p}"))
            .collect::<Vec<_>>()
            .join(" \\\n");
        out.push_str(&format!(
            "RUN uv pip install --system --target=/deps \\\n{pkgs}\n\n"
        ));
    }

    // Stage 2: apt install
    let apt_list = ALWAYS_APT.iter().copied().collect::<BTreeSet<_>>();
    // Include skill apt deps too (they're already in bundle_paths, but we need them installed here)
    // We just install everything since apt_deps = ALWAYS_APT + skill deps
    let all_apt_str = apt_list.into_iter().collect::<Vec<_>>().join(" \\\n    ");
    out.push_str("FROM debian:bookworm-slim AS apt-builder\n");
    out.push_str(&format!(
        "RUN apt-get update && apt-get install -y --no-install-recommends \\\n    {all_apt_str} \\\n    && rm -rf /var/lib/apt/lists/*\n\n"
    ));

    // Stage 3: collector — bundle all paths into /bundle
    out.push_str("FROM debian:bookworm-slim AS collector\n");
    out.push_str("COPY --from=apt-builder / /\n");
    let cp_cmds = bundle_paths
        .iter()
        .filter(|p| !p.ends_with('/')) // skip directory-only entries
        .map(|p| format!("    cp --parents {p} /bundle 2>/dev/null || true"))
        .collect::<Vec<_>>()
        .join(" \\\n");
    if cp_cmds.is_empty() {
        out.push_str("RUN mkdir -p /bundle\n\n");
    } else {
        out.push_str(&format!("RUN mkdir -p /bundle \\\n{cp_cmds}\n\n"));
    }

    // Final stage: distroless
    let base = if has_python {
        "gcr.io/distroless/python3-debian12"
    } else {
        "gcr.io/distroless/cc-debian12"
    };
    out.push_str(&format!("FROM {base}\n"));
    out.push_str("COPY --from=collector /bundle/ /\n");

    if has_python && !uv_deps.is_empty() {
        out.push_str("COPY --from=uv-builder /deps /deps\n");
        out.push_str("ENV PYTHONPATH=/deps\n");
        out.push_str("ENV PATH=\"/deps/bin:/usr/local/bin:/usr/bin:/bin\"\n");
    }

    out.push_str("COPY --chmod=755 tamad /tamad\n");
    out.push_str("COPY agents/ /app/agents/\n");
    out.push_str("COPY skills/ /app/skills/\n");
    out.push_str("COPY tama.toml /app/tama.toml\n");
    out.push_str("WORKDIR /app\n");

    if !entrypoint.is_empty() {
        out.push_str(&format!("ENV TAMA_ENTRYPOINT_AGENT={entrypoint}\n"));
    }

    out.push_str("ENTRYPOINT [\"/tamad\"]\n");

    out
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn copy_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
