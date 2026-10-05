//! The private AI runtime inside the bundle: `tools/ai-runtime/build.py`
//! assembles a relocatable CPython with the locked wheel set, the pinned LTX
//! source, the worker adapter and GPL `ffmpeg`/`ffprobe` from verified inputs
//! (cached under the build-input cache). This module copies it to
//! `Contents/Resources/ai-runtime`, signs every nested Mach-O, writes its
//! notices and SBOM components, and checks it runs from the staged bundle.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use super::{Result, macho, notices, run_tool, sha256_file, spdx};

/// Below `Contents`.
pub const DIRECTORY: &str = "Resources/ai-runtime";
const IDENTIFIER_PREFIX: &str = "dev.deadpan.Deadpan.ai";

/// An assembled runtime in the build-input cache.
pub struct Built {
    pub runtime: PathBuf,
    pub notices: PathBuf,
    pub report: Value,
}

/// Build (or reuse) the runtime for these pins.
pub fn build(workspace: &Path, cache: &Path, ltx_checkout: Option<&Path>) -> Result<Built> {
    let script = workspace.join("tools/ai-runtime/build.py");
    let mut command = Command::new("python3");
    command
        .arg(&script)
        .arg("build")
        .arg("--cache")
        .arg(cache)
        // The runtime's FFmpeg is a separate, pinned GPL build; the LGPL
        // developer prefix must not leak into it.
        .env_remove("DEADPAN_FFMPEG_PREFIX");
    if let Some(checkout) = ltx_checkout {
        command.arg("--ltx-checkout").arg(checkout);
    }
    let result = command
        .output()
        .map_err(|e| format!("failed to start {}: {e}", script.display()))?;
    if !result.status.success() {
        return Err(format!(
            "AI runtime build failed: {}{}",
            String::from_utf8_lossy(&result.stdout).trim(),
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&result.stdout);
    let line = stdout
        .lines()
        .rev()
        .find(|line| line.starts_with('{'))
        .ok_or("the AI runtime build printed no result")?;
    let located: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    let path = |key: &str| {
        located[key]
            .as_str()
            .map(PathBuf::from)
            .ok_or_else(|| format!("the AI runtime build did not report {key}"))
    };
    let report: Value = serde_json::from_slice(
        &fs::read(path("report")?).map_err(|e| format!("AI runtime report: {e}"))?,
    )
    .map_err(|e| e.to_string())?;
    Ok(Built {
        runtime: path("runtime")?,
        notices: path("notices")?,
        report,
    })
}

/// Copy the runtime into the staged bundle, keeping its symbolic links.
pub fn install(built: &Built, contents: &Path) -> Result<PathBuf> {
    let destination = contents.join(DIRECTORY);
    run_tool(
        "ditto",
        &[built.runtime.as_os_str(), destination.as_os_str()],
    )?;
    Ok(destination)
}

/// Every Mach-O file below `directory`, libraries before executables, so
/// signing proceeds inside out.
pub fn code(directory: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let mut libraries = Vec::new();
    let mut executables = Vec::new();
    for path in macho::bundle_files(directory)? {
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() || !macho::is_macho(&path)? {
            continue;
        }
        if macho::is_executable(&path)? {
            executables.push(path);
        } else {
            libraries.push(path);
        }
    }
    Ok((libraries, executables))
}

/// Sign every nested library, then each executable with its own identifier.
pub fn sign(
    directory: &Path,
    sign: &dyn Fn(&Path, Option<&str>) -> Result<()>,
) -> Result<(usize, usize)> {
    let (libraries, executables) = code(directory)?;
    for library in &libraries {
        sign(library, None)?;
    }
    for executable in &executables {
        let name = executable
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("non-UTF-8 executable name")?;
        sign(executable, Some(&format!("{IDENTIFIER_PREFIX}.{name}")))?;
    }
    Ok((libraries.len(), executables.len()))
}

/// Run the staged runtime as the host will, in a cleared environment under
/// the hardened runtime: Python imports MLX and the LTX pipeline and runs a
/// Metal calculation, and FFmpeg reports libx264.
pub fn check_staged(runtime: &Path, scratch: &Path) -> Result<String> {
    let runtime = fs::canonicalize(runtime).map_err(|e| format!("{}: {e}", runtime.display()))?;
    let home = scratch.join("ai-runtime-home");
    fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    // Prints the prefix and where the native and LTX modules came from.
    let check = "import sys, os, mlx.core as mx, numpy, PIL, mlx_lm, safetensors, ltx_core_mlx, \
ltx_pipelines_mlx.keyframe_interpolation as k; a = mx.arange(10); \
assert int((a*a).sum().item()) == 285; \
print(os.path.realpath(sys.prefix)); print(os.path.realpath(mx.__file__)); \
print(os.path.realpath(k.__file__)); print(mx.default_device())";
    let run = |program: &Path, arguments: &[&str]| -> Result<String> {
        let result = Command::new(program)
            .args(arguments)
            .env_clear()
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin")
            .current_dir(&home)
            .output()
            .map_err(|e| format!("failed to start {}: {e}", program.display()))?;
        if !result.status.success() {
            return Err(format!(
                "{} {} failed: {}",
                program.display(),
                arguments.join(" "),
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&result.stdout).into_owned())
    };
    let printed = run(
        &runtime.join("python/bin/python3.12"),
        &["-I", "-B", "-c", check],
    )?;
    let lines: Vec<&str> = printed.lines().collect();
    let [prefix, mlx, pipeline, device] = lines.as_slice() else {
        return Err(format!(
            "the bundled Python printed an unexpected report: {printed}"
        ));
    };
    for (label, path, inside) in [
        ("prefix", prefix, runtime.join("python")),
        ("MLX", mlx, runtime.join("python")),
        ("LTX pipeline", pipeline, runtime.join("ltx-2-mlx")),
    ] {
        if !Path::new(path).starts_with(&inside) {
            return Err(format!(
                "the bundled Python resolved its {label} outside {}: {path}",
                inside.display()
            ));
        }
    }
    if !device.contains("gpu") {
        return Err(format!("the bundled MLX did not select the GPU: {device}"));
    }
    let encoders = run(&runtime.join("bin/ffmpeg"), &["-hide_banner", "-encoders"])?;
    if !encoders.contains("libx264rgb") {
        return Err("the bundled ffmpeg lacks libx264rgb".into());
    }
    run(&runtime.join("bin/ffprobe"), &["-hide_banner", "-version"])?;
    Ok(format!("{prefix} {device}"))
}

/// A wheel or source component from the build report.
struct Component {
    name: String,
    version: String,
    license: String,
    reference: String,
    sha256: Option<String>,
    url: String,
}

fn components(report: &Value) -> Vec<Component> {
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    let mut items = vec![Component {
        name: "CPython (python-build-standalone)".into(),
        version: format!(
            "{}+{}",
            text(&report["python"]["version"]),
            text(&report["python"]["release"])
        ),
        license: text(&report["python"]["license"]),
        reference: "ai:cpython".into(),
        sha256: Some(text(&report["python"]["sha256"])),
        url: text(&report["python"]["url"]),
    }];
    for wheel in report["wheels"].as_array().into_iter().flatten() {
        items.push(Component {
            name: text(&wheel["name"]),
            version: text(&wheel["version"]),
            license: text(&wheel["license"]),
            reference: format!("ai:pypi:{}", text(&wheel["name"])),
            sha256: Some(text(&wheel["sha256"])),
            url: text(&wheel["url"]),
        });
    }
    let source = &report["ltx_source"];
    items.push(Component {
        name: "ltx-2-mlx (ltx-core-mlx, ltx-pipelines-mlx)".into(),
        version: text(&source["commit"]),
        license: text(&source["license"]),
        reference: "ai:ltx-2-mlx".into(),
        sha256: None,
        url: format!(
            "{}/tree/{}",
            text(&source["repository"]),
            text(&source["commit"])
        ),
    });
    items.push(Component {
        name: "x264".into(),
        version: format!(
            "{} ({})",
            text(&report["x264"]["version"]),
            text(&report["x264"]["commit"])
        ),
        license: text(&report["x264"]["license"]),
        reference: "ai:x264".into(),
        sha256: None,
        url: text(&report["x264"]["repository"]),
    });
    items.push(Component {
        name: "FFmpeg programs ffmpeg and ffprobe (GPL build with libx264)".into(),
        version: text(&report["ffmpeg"]["version"]),
        license: text(&report["ffmpeg"]["license"]),
        reference: "ai:ffmpeg-gpl".into(),
        sha256: Some(text(&report["ffmpeg"]["archive_sha256"])),
        url: text(&report["ffmpeg"]["archive_url"]),
    });
    items
}

/// Copy the collected license files and write `AI_RUNTIME_NOTICES.txt`.
pub fn write_notices(built: &Built, workspace: &Path, notices_directory: &Path) -> Result<()> {
    let identifiers = notices::spdx_identifiers(workspace)?;
    for component in components(&built.report) {
        spdx::parse(&component.license, &identifiers).map_err(|error| {
            format!(
                "AI runtime component {} has an invalid license expression {}: {error}",
                component.name, component.license
            )
        })?;
    }
    let destination = notices_directory.join("ai-runtime");
    run_tool(
        "ditto",
        &[built.notices.as_os_str(), destination.as_os_str()],
    )?;
    let rule = "=".repeat(78);
    let report = &built.report;
    let mut text = String::new();
    let _ = writeln!(
        text,
        "Deadpan private AI runtime notices\n\n\
Contents/Resources/ai-runtime holds the separate programs Deadpan runs to\n\
generate AI pause pictures: a CPython interpreter with the Python packages\n\
below, the ltx-2-mlx source, a worker adapter and ffmpeg/ffprobe. They run as\n\
separate processes; none is linked into Deadpan. The license files each\n\
package ships are in Notices/ai-runtime/. Model weights are not part of the\n\
application; their licenses are shown and accepted when a model pack is\n\
installed.\n\n\
Runtime {} {}, assembled from pinned inputs (tools/ai-runtime/pins.json).\n",
        report["runtime_id"].as_str().unwrap_or_default(),
        report["runtime_version"].as_str().unwrap_or_default(),
    );
    let _ = writeln!(
        text,
        "{rule}\nffmpeg and ffprobe (Contents/Resources/ai-runtime/bin)\n{rule}\n\
These two programs are a GPL build: FFmpeg {} configured with --enable-gpl\n\
--enable-libx264 and statically linked with x264 {} (commit {}), both under\n\
the GNU General Public License version 2 or later (ai-runtime/ffmpeg-COPYING.GPLv2,\n\
ai-runtime/x264-COPYING). The AI worker uses them for the model's H.264\n\
conditioning preprocessing and lossless intermediates. They are separate\n\
executables, distinct from the LGPL FFmpeg libraries in Contents/Frameworks.\n\n\
Corresponding source: {} (SHA-256 {}) and {} at commit {} (tree {}).\n\
Build configuration: {}\n",
        report["ffmpeg"]["version"].as_str().unwrap_or_default(),
        report["x264"]["version"].as_str().unwrap_or_default(),
        report["x264"]["commit"].as_str().unwrap_or_default(),
        report["ffmpeg"]["archive_url"].as_str().unwrap_or_default(),
        report["ffmpeg"]["archive_sha256"]
            .as_str()
            .unwrap_or_default(),
        report["x264"]["repository"].as_str().unwrap_or_default(),
        report["x264"]["commit"].as_str().unwrap_or_default(),
        report["x264"]["tree"].as_str().unwrap_or_default(),
        report["ffmpeg"]["configuration"]
            .as_str()
            .unwrap_or_default(),
    );
    let stripped = report["stripped_rpaths"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !stripped.is_empty() {
        let _ = writeln!(
            text,
            "Modified files: absolute LC_RPATH entries left by upstream wheel builds were\n\
removed (install_name_tool -delete_rpath) so no library loads from outside the\n\
bundle; the code is otherwise as published, then re-signed:"
        );
        for entry in &stripped {
            let _ = writeln!(
                text,
                "  {} (removed {})",
                entry["file"].as_str().unwrap_or_default(),
                entry["rpath"].as_str().unwrap_or_default()
            );
        }
        text.push('\n');
    }
    let _ = writeln!(text, "{rule}\nComponents\n{rule}");
    for component in components(report) {
        let _ = writeln!(
            text,
            "{} {}\n  License: {}\n  Source: {}{}",
            component.name,
            component.version,
            component.license,
            component.url,
            component
                .sha256
                .map(|sha| format!("\n  SHA-256: {sha}"))
                .unwrap_or_default()
        );
    }
    fs::write(notices_directory.join("AI_RUNTIME_NOTICES.txt"), text).map_err(|e| e.to_string())
}

/// CycloneDX components for the runtime, with shipped executable hashes.
pub fn sbom_components(built: &Built, runtime: &Path) -> Result<Vec<Value>> {
    let mut items: Vec<Value> = components(&built.report)
        .into_iter()
        .map(|component| {
            let mut value = json!({
                "type": "library",
                "bom-ref": component.reference,
                "name": component.name,
                "version": component.version,
                "licenses": [{ "expression": component.license }],
                "externalReferences": [{ "type": "distribution", "url": component.url }],
                "properties": [{ "name": "deadpan:location", "value": "Contents/Resources/ai-runtime" }],
            });
            if let Some(sha) = component.sha256 {
                value["hashes"] = json!([{ "alg": "SHA-256", "content": sha }]);
            }
            value
        })
        .collect();
    for program in ["python/bin/python3.12", "bin/ffmpeg", "bin/ffprobe"] {
        items.push(json!({
            "type": "file",
            "name": format!("Contents/Resources/ai-runtime/{program}"),
            "hashes": [{ "alg": "SHA-256", "content": sha256_file(&runtime.join(program))? }],
        }));
    }
    Ok(items)
}
