//! mcpack: turns a Minecraft 26.3 folder (Prism/MultiMC instance or official launcher dir)
//! plus its client jar into one `.hzpack` file for the Horizon web launcher.
//!
//! Layout next to mcpack.exe:
//!   prep/horizon-prepare.exe, prep/research-lock.json, prep/<exporter sources>
//! Usage: mcpack.exe [MINECRAFT_FOLDER] [CLIENT_JAR] [--sound 1-4] [--java-home DIR] [--out FILE]
//!        mcpack.exe EXISTING.hzpack [--sound 1-4] [--out FILE]   (re-pack sounds only, no Java)
//! With no arguments it asks with dialogs.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod sound;
mod web;
use sound::Mode;

const PACK_FORMAT: u32 = 1;

fn main() {
    let code = match run() {
        Ok(path) => {
            println!("\nDone: {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("\nError: {e}");
            1
        }
    };
    if !std::env::args().any(|a| a == "--sound") {
        print!("\nPress Enter to close...");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    std::process::exit(code);
}

struct Args {
    folder: Option<PathBuf>,
    jar: Option<PathBuf>,
    java_home: Option<PathBuf>,
    out: Option<PathBuf>,
    sound: Option<Mode>,
    repack: Option<PathBuf>,
    web: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { folder: None, jar: None, java_home: None, out: None, sound: None, repack: None, web: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--java-home" => a.java_home = Some(it.next().ok_or("--java-home needs a path")?.into()),
            "--sound" => {
                let n = it.next().ok_or("--sound needs 1-4")?;
                a.sound = Some(n.parse().ok().and_then(Mode::from_num).ok_or("--sound needs 1-4")?);
            }
            "--web" => a.web = true,
            "--out" => a.out = Some(it.next().ok_or("--out needs a path")?.into()),
            "-h" | "--help" => {
                println!("mcpack [MINECRAFT_FOLDER] [CLIENT_JAR] [--sound 1-4] [--java-home DIR] [--out FILE]\nmcpack EXISTING.hzpack [--sound 1-4] [--out FILE]\nmcpack --web   (Launch Web)");
                std::process::exit(0);
            }
            _ => {
                let p = PathBuf::from(&arg);
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hzpack")) {
                    a.repack = Some(p);
                } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jar")) {
                    a.jar = Some(p);
                } else {
                    a.folder = Some(p);
                }
            }
        }
    }
    Ok(a)
}

fn run() -> Result<PathBuf, String> {
    println!("mcpack {} - Minecraft 26.3 -> .hzpack\n", env!("CARGO_PKG_VERSION"));
    let mut args = parse_args()?;
    let exe_dir0 = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("no exe folder")?
        .to_path_buf();
    if std::env::args().len() <= 1 {
        args.web = ask_task();
    }
    if args.web {
        web::launch(exe_dir0.join("web"), true)?;
        return Ok(exe_dir0.join("web"));
    }
    let interactive = std::env::args().len() <= 1 || args.sound.is_none();
    let mode = match args.sound {
        Some(m) => m,
        None if interactive => ask_sound(),
        None => Mode::Original,
    };

    if let Some(src) = &args.repack {
        let out = args.out.clone().unwrap_or_else(|| {
            let stem = src.file_stem().unwrap_or_default().to_string_lossy();
            src.with_file_name(format!("{stem}-s{}.hzpack", mode as u32))
        });
        println!("Re-packing {} (sounds: {})", src.display(), mode.name());
        let count = repack(src, &out, mode)?;
        println!("{count} files ({:.1} MB)", fs::metadata(&out).map(|m| m.len()).unwrap_or(0) as f64 / 1e6);
        return Ok(out);
    }

    let exe_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("no exe folder")?
        .to_path_buf();
    let prep_root = exe_dir.join("prep");
    let prepare = prep_root.join(if cfg!(windows) { "horizon-prepare.exe" } else { "horizon-prepare" });
    if !prepare.is_file() || !prep_root.join("research-lock.json").is_file() {
        return Err(format!(
            "missing {} (keep the prep folder next to mcpack)",
            prepare.display()
        ));
    }

    let folder = match args.folder {
        Some(f) => f,
        None => pick_folder().ok_or("no folder picked")?,
    };
    let jar = match args.jar {
        Some(j) => Some(j),
        None => pick_jar(),
    };
    if !folder.is_dir() {
        return Err(format!("not a folder: {}", folder.display()));
    }
    if let Some(j) = &jar {
        if !j.is_file() {
            return Err(format!("not a file: {}", j.display()));
        }
    }

    let lock: serde_json::Value = serde_json::from_slice(
        &fs::read(prep_root.join("research-lock.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let version = lock["minecraft"]["version"].as_str().unwrap_or("26.3").to_string();

    let work = exe_dir.join(".work");
    let out_dir = work.join(format!("resources-{version}"));
    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).map_err(|e| format!("clearing old output: {e}"))?;
    }
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;

    // Prism/MultiMC instances are detected from the folder; otherwise treat it as the official dir.
    let prism = folder.ancestors().take(5).any(|d| d.join("mmc-pack.json").is_file())
        || folder.join("instances").is_dir();

    let mut cmd = Command::new(&prepare);
    cmd.arg("--prepare")
        .arg(if prism { "--prism-dir" } else { "--minecraft-dir" })
        .arg(&folder)
        .arg("--root")
        .arg(&prep_root)
        .arg("--output")
        .arg(&out_dir)
        .args(["--complete", "--progress", "json"]);
    if let Some(j) = &jar {
        cmd.arg("--client-jar").arg(j);
    }
    if let Some(j) = &args.java_home {
        cmd.arg("--java-home").arg(j);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    println!("Folder: {} ({})", folder.display(), if prism { "Prism/MultiMC" } else { "official launcher" });
    if let Some(j) = &jar {
        println!("Jar:    {}", j.display());
    }
    println!("Preparing Minecraft {version} resources (needs Java 25; first run takes a few minutes)...\n");

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("starting horizon-prepare: {e}"))?;
    let stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut all = String::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            all.push_str(&line);
            all.push('\n');
        }
        all
    });
    let mut last_error = None;
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(v) if v.get("fraction").is_some() => {
                let f = v["fraction"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
                let n = (f * 30.0) as usize;
                print!("\r[{}{}] {:>3}%", "#".repeat(n), ".".repeat(30 - n), (f * 100.0) as u32);
                let _ = std::io::stdout().flush();
            }
            Ok(v) if v.get("message").is_some() => {
                println!("\r{:<60}", v["message"].as_str().unwrap_or(""));
            }
            Ok(v) if v.get("error").is_some() => {
                last_error = v["error"].as_str().map(str::to_string);
            }
            _ => println!("{line}"),
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let stderr_text = err_thread.join().unwrap_or_default();
    if !status.success() {
        return Err(last_error.unwrap_or_else(|| {
            if stderr_text.trim().is_empty() {
                format!("horizon-prepare failed ({status})")
            } else {
                stderr_text.trim().to_string()
            }
        }));
    }

    let pack = args
        .out
        .unwrap_or_else(|| exe_dir.join(format!("minecraft-{version}.hzpack")));
    println!("\nPacking into {} ...", pack.display());
    let count = write_pack(&out_dir, &pack, &version, &lock, mode)?;
    println!("{count} files packed ({:.1} MB)", fs::metadata(&pack).map(|m| m.len()).unwrap_or(0) as f64 / 1e6);
    let _ = fs::remove_dir_all(&out_dir);
    Ok(pack)
}

/// The pack is a zip: `hzpack.json` (manifest) plus the prepared resource tree under `res/`.
fn write_pack(dir: &Path, pack: &Path, version: &str, lock: &serde_json::Value, mode: Mode) -> Result<usize, String> {
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(dir).unwrap();
            let name = format!("res/{}", rel.to_string_lossy().replace('\\', "/"));
            entries.push((name, fs::read(entry.path()).map_err(|e| e.to_string())?));
        }
    }
    let manifest = serde_json::json!({
        "format": PACK_FORMAT,
        "minecraft": version,
        "client_sha1": lock["minecraft"]["client_sha1"],
    });
    emit(entries, manifest, pack, mode)
}

/// Re-packs an existing .hzpack with another sound mode.
fn repack(src: &Path, pack: &Path, mode: Mode) -> Result<usize, String> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(File::open(src).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut manifest = serde_json::json!({ "format": PACK_FORMAT });
    let mut entries = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().to_string();
        let mut data = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut data).map_err(|e| e.to_string())?;
        if name == "hzpack.json" {
            manifest = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        } else {
            entries.push((name, data));
        }
    }
    emit(entries, manifest, pack, mode)
}

/// Converts the .ogg entries in parallel, then writes the zip.
/// Already-compressed files (png, ogg, nbt) are stored, the rest deflated.
fn emit(mut entries: Vec<(String, Vec<u8>)>, mut manifest: serde_json::Value, pack: &Path, mode: Mode) -> Result<usize, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zip::write::SimpleFileOptions;
    use zip::CompressionMethod;

    let oggs: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].0.ends_with(".ogg")).collect();
    let before: u64 = oggs.iter().map(|&i| entries[i].1.len() as u64).sum();
    if mode != Mode::Original && !oggs.is_empty() {
        println!("Converting {} sounds ({})...", oggs.len(), mode.name());
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let total = oggs.len();
        let results: Vec<std::sync::Mutex<Option<Vec<u8>>>> = (0..total).map(|_| std::sync::Mutex::new(None)).collect();
        std::thread::scope(|sc| {
            for _ in 0..threads {
                sc.spawn(|| loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= total {
                        break;
                    }
                    let out = sound::convert(&entries[oggs[k]].1, mode);
                    *results[k].lock().unwrap() = out;
                    let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if d % 50 == 0 || d == total {
                        let n = d * 30 / total;
                        print!("\r[{}{}] {d}/{total}", "#".repeat(n), ".".repeat(30 - n));
                        let _ = std::io::stdout().flush();
                    }
                });
            }
        });
        println!();
        for (k, r) in results.into_iter().enumerate() {
            if let Some(data) = r.into_inner().unwrap() {
                entries[oggs[k]].1 = data;
            }
        }
        let after: u64 = oggs.iter().map(|&i| entries[i].1.len() as u64).sum();
        println!("Sounds: {:.1} MB -> {:.1} MB", before as f64 / 1e6, after as f64 / 1e6);
    }

    let tmp = pack.with_extension("hzpack.part");
    let file = File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let mut z = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let mut total = 0u64;
    let count = entries.len();
    for (name, data) in &entries {
        let stored = [".png", ".ogg", ".nbt", ".zip", ".jar", ".gz"].iter().any(|e| name.ends_with(e));
        let opts = SimpleFileOptions::default()
            .large_file(true)
            .compression_method(if stored { CompressionMethod::Stored } else { CompressionMethod::Deflated });
        z.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
        z.write_all(data).map_err(|e| e.to_string())?;
        total += data.len() as u64;
    }
    manifest["files"] = count.into();
    manifest["bytes"] = total.into();
    manifest["sound"] = (mode as u32).into();
    manifest["tool"] = concat!("mcpack ", env!("CARGO_PKG_VERSION")).into();
    z.start_file("hzpack.json", SimpleFileOptions::default()).map_err(|e| e.to_string())?;
    z.write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;
    z.finish().map_err(|e| e.to_string())?;
    fs::rename(&tmp, pack).map_err(|e| e.to_string())?;
    Ok(count)
}

/// true = Launch Web, false = make a .hzpack.
fn ask_task() -> bool {
    println!("What do you want to do?");
    println!("  1 = make a .hzpack from your Minecraft 26.3");
    println!("  2 = Launch Web (play in your browser)");
    loop {
        print!("Pick 1-2 [2]: ");
        let _ = std::io::stdout().flush();
        let mut s = String::new();
        if std::io::stdin().read_line(&mut s).is_err() {
            return true;
        }
        match s.trim() {
            "" | "2" => return true,
            "1" => {
                println!();
                return false;
            }
            _ => {}
        }
    }
}

fn ask_sound() -> Mode {
    println!("Sound options:");
    println!("  1 = original (biggest)");
    println!("  2 = compress a bit");
    println!("  3 = compress a LOT (mono, low quality)");
    println!("  4 = no sound (silent files, same length)");
    loop {
        print!("Pick 1-4 [1]: ");
        let _ = std::io::stdout().flush();
        let mut s = String::new();
        if std::io::stdin().read_line(&mut s).is_err() {
            return Mode::Original;
        }
        let s = s.trim();
        if s.is_empty() {
            return Mode::Original;
        }
        if let Some(m) = s.parse().ok().and_then(Mode::from_num) {
            println!("Sound: {}\n", m.name());
            return m;
        }
    }
}

#[cfg(windows)]
fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Pick your 26.3 .minecraft / instance 'minecraft' folder")
        .pick_folder()
}
#[cfg(windows)]
fn pick_jar() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Pick minecraft-26.3-client.jar (Cancel = find it automatically)")
        .add_filter("Client jar", &["jar"])
        .pick_file()
}
#[cfg(not(windows))]
fn ask(q: &str) -> Option<PathBuf> {
    print!("{q}: ");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    std::io::stdin().read_line(&mut s).ok()?;
    let s = s.trim().trim_matches('"');
    (!s.is_empty()).then(|| PathBuf::from(s))
}
#[cfg(not(windows))]
fn pick_folder() -> Option<PathBuf> { ask("Minecraft folder") }
#[cfg(not(windows))]
fn pick_jar() -> Option<PathBuf> { ask("Client jar (empty = auto)") }
