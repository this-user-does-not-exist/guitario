//! `cargo xtask package [--target <triple>] [--no-build]`: builds Guitario in release and lays out,
//! in `dist/`, what the release workflow uploads (`.github/workflows/release.yml`):
//!
//! - macOS: `Guitario.app` (both binaries, the icon, `packaging/macos/Info.plist` with the document
//!   types and the microphone use the tuner needs), zipped with `ditto`, and a `.dmg` with `hdiutil`.
//! - Linux: a `.tar.gz` (binaries, desktop entry, MIME types, AppStream metadata, icons,
//!   `install.sh`) and, where `dpkg-deb` is installed, a `.deb`.
//! - Windows: a `.zip` and, where Inno Setup's `ISCC` is installed, the installer
//!   (`packaging/windows/guitario.iss`), which also registers the file types.
//!
//! Signing happens here when its environment variables are set (the release workflow sets them
//! from its secrets): `GUITARIO_SIGN_IDENTITY` and `GUITARIO_NOTARY_*` on macOS
//! (`package_macos`), `GUITARIO_WINDOWS_CERT*` on Windows (`sign_windows`). Without them the
//! packages are built unsigned.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Os {
    Mac,
    Linux,
    Windows,
}

impl Os {
    fn of(triple: &str) -> Option<Os> {
        if triple.contains("apple-darwin") {
            Some(Os::Mac)
        } else if triple.contains("windows") {
            Some(Os::Windows)
        } else if triple.contains("linux") {
            Some(Os::Linux)
        } else {
            None
        }
    }

    fn host() -> Os {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux
        }
    }

    fn exe(self, name: &str) -> String {
        if self == Os::Windows { format!("{name}.exe") } else { name.to_string() }
    }
}

const USAGE: &str = "usage: cargo xtask package [--target <triple>] [--no-build]";

fn sh(mut c: Command, what: &str) -> Result<(), String> {
    eprintln!("==> {what}");
    let status = c.status().map_err(|e| format!("{what}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{what} failed")) }
}

fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok()
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

fn write(to: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(to, text).map_err(|e| format!("{}: {e}", to.display()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    for e in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?.flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()))?;
        } else {
            copy(&p, &to.join(e.file_name()))?;
        }
    }
    Ok(())
}

/// A packaging template from `packaging/` with `@VERSION@`, `@BUNDLE_VERSION@` (without a
/// pre-release part: macOS wants numbers only) and `@DATE@` (the release's day) filled in.
fn template(root: &Path, rel: &str, version: &str) -> Result<String, String> {
    let path = root.join("packaging").join(rel);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let numbers = version.split_once('-').map_or(version, |(core, _)| core);
    Ok(text.replace("@VERSION@", version).replace("@BUNDLE_VERSION@", numbers).replace("@DATE@", &release_date(root)))
}

/// The day of the commit being packaged (so a rebuild is the same), or today outside git.
fn release_date(root: &Path) -> String {
    let git = Command::new("git").args(["log", "-1", "--format=%cs"]).current_dir(root).output();
    match git.ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()) {
        Some(day) if day.len() == 10 => day,
        _ => {
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            civil(secs / 86_400)
        }
    }
}

/// `YYYY-MM-DD` of a day counted from 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil(days: u64) -> String {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + u64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Debian's name for an architecture.
fn deb_arch(arch: &str) -> &str {
    match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

/// The `DEBIAN/control` file of the `.deb`. The app links only ALSA; windowing and GPU libraries
/// are loaded when it starts, and curl installs the sound libraries.
fn deb_control(version: &str, arch: &str, size_kb: u64) -> String {
    format!(
        "Package: guitario\nVersion: {version}\nSection: sound\nPriority: optional\nArchitecture: {}\nInstalled-Size: {size_kb}\n\
         Maintainer: Guitario contributors <https://github.com/this-user-does-not-exist/guitario>\nHomepage: https://github.com/this-user-does-not-exist/guitario\n\
         Depends: libasound2t64 | libasound2\nRecommends: libxkbcommon0, libvulkan1 | libegl1, curl\nSuggests: git\n\
         Description: Guitar tablature and notation editor\n Write, play and print guitar tablature and standard notation; opens and saves\n Guitar Pro files, imports and exports MIDI and MusicXML.\n",
        deb_arch(arch)
    )
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir).map_or(0, |rd| rd.flatten().map(|e| if e.path().is_dir() { dir_size(&e.path()) } else { e.metadata().map_or(0, |m| m.len()) }).sum())
}

/// Lays out the Linux files under `share` (desktop entry, MIME types, metainfo, icons).
fn linux_share(root: &Path, share: &Path, version: &str) -> Result<(), String> {
    let linux = root.join("packaging/linux");
    copy(&linux.join("ai.storyteller.guitario.desktop"), &share.join("applications/ai.storyteller.guitario.desktop"))?;
    copy(&linux.join("guitario-mime.xml"), &share.join("mime/packages/guitario-mime.xml"))?;
    write(&share.join("metainfo/ai.storyteller.guitario.metainfo.xml"), &template(root, "linux/ai.storyteller.guitario.metainfo.xml", version)?)?;
    copy_dir(&root.join("assets/app-icon/hicolor"), &share.join("icons/hicolor"))
}

fn package_linux(root: &Path, bins: &Path, dist: &Path, version: &str, arch: &str) -> Result<Vec<PathBuf>, String> {
    let name = format!("guitario-{version}-linux-{arch}");
    let dir = dist.join(&name);
    let _ = std::fs::remove_dir_all(&dir);
    for b in ["guitario", "guitario-cli"] {
        copy(&bins.join(b), &dir.join("bin").join(b))?;
    }
    linux_share(root, &dir.join("share"), version)?;
    copy(&root.join("packaging/linux/install.sh"), &dir.join("install.sh"))?;
    for f in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "README.md"] {
        copy(&root.join(f), &dir.join(f))?;
    }
    let tarball = dist.join(format!("{name}.tar.gz"));
    let mut c = Command::new("tar");
    c.arg("-czf").arg(&tarball).arg("-C").arg(dist).arg(&name);
    sh(c, "tar")?;
    let mut out = vec![tarball];
    if have("dpkg-deb") {
        let deb = dist.join(format!("guitario_{version}_{}", deb_arch(arch)));
        let _ = std::fs::remove_dir_all(&deb);
        for b in ["guitario", "guitario-cli"] {
            copy(&bins.join(b), &deb.join("usr/bin").join(b))?;
        }
        linux_share(root, &deb.join("usr/share"), version)?;
        for f in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE"] {
            copy(&root.join(f), &deb.join("usr/share/doc/guitario").join(f))?;
        }
        let size = dir_size(&deb) / 1024;
        write(&deb.join("DEBIAN/control"), &deb_control(version, arch, size))?;
        let file = dist.join(format!("guitario_{version}_{}.deb", deb_arch(arch)));
        let mut c = Command::new("dpkg-deb");
        c.args(["--build", "--root-owner-group"]).arg(&deb).arg(&file);
        sh(c, "dpkg-deb")?;
        let _ = std::fs::remove_dir_all(&deb);
        out.push(file);
    } else {
        eprintln!("==> dpkg-deb not found: no .deb");
    }
    Ok(out)
}

/// An environment variable that is set and not blank.
fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn codesign(identity: &str, path: &Path, entitlements: Option<&Path>) -> Result<(), String> {
    let mut c = Command::new("codesign");
    c.args(["--force", "--timestamp", "--sign", identity]);
    if let Some(e) = entitlements {
        c.args(["--options", "runtime", "--entitlements"]).arg(e);
    }
    c.arg(path);
    sh(c, &format!("codesign {}", path.display()))
}

/// Sends `path` to Apple's notary service and staples the ticket to `staple`; false when the
/// credentials are not set. A rejected submission fails at the staple.
fn notarize(path: &Path, staple: &Path) -> Result<bool, String> {
    let (Some(id), Some(team), Some(password)) = (env("GUITARIO_NOTARY_APPLE_ID"), env("GUITARIO_NOTARY_TEAM_ID"), env("GUITARIO_NOTARY_PASSWORD")) else {
        return Ok(false);
    };
    let mut c = Command::new("xcrun");
    c.args(["notarytool", "submit"]).arg(path).args(["--apple-id", &id, "--team-id", &team, "--password", &password, "--wait"]);
    sh(c, &format!("notarize {}", path.display()))?;
    let mut c = Command::new("xcrun");
    c.args(["stapler", "staple"]).arg(staple);
    sh(c, &format!("staple {}", staple.display()))?;
    Ok(true)
}

fn ditto_zip(app: &Path, zip: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(zip);
    let mut c = Command::new("ditto");
    c.args(["-c", "-k", "--sequesterRsrc", "--keepParent"]).arg(app).arg(zip);
    sh(c, "ditto")
}

/// The app bundle, zipped and as a disk image. With `GUITARIO_SIGN_IDENTITY` (a Developer ID
/// Application certificate in the keychain) both binaries are signed for the hardened runtime
/// with `packaging/macos/entitlements.plist`; with the notary credentials as well, the app and
/// the disk image are notarized and stapled, so Gatekeeper opens them offline.
fn package_macos(root: &Path, bins: &Path, dist: &Path, version: &str, arch: &str) -> Result<Vec<PathBuf>, String> {
    let app = dist.join("Guitario.app");
    let _ = std::fs::remove_dir_all(&app);
    let contents = app.join("Contents");
    for b in ["guitario", "guitario-cli"] {
        copy(&bins.join(b), &contents.join("MacOS").join(b))?;
    }
    copy(&root.join("assets/app-icon/guitario.icns"), &contents.join("Resources/guitario.icns"))?;
    write(&contents.join("Info.plist"), &template(root, "macos/Info.plist", version)?)?;
    write(&contents.join("PkgInfo"), "APPL????")?;
    let identity = env("GUITARIO_SIGN_IDENTITY");
    if let Some(id) = &identity {
        let entitlements = root.join("packaging/macos/entitlements.plist");
        codesign(id, &contents.join("MacOS/guitario-cli"), Some(&entitlements))?;
        codesign(id, &app, Some(&entitlements))?;
    } else {
        eprintln!("==> GUITARIO_SIGN_IDENTITY not set: the app is not signed");
    }
    let base = format!("Guitario-{version}-macos-{arch}");
    let zip = dist.join(format!("{base}.zip"));
    ditto_zip(&app, &zip)?;
    // The notary service takes the zip; the ticket is stapled to the app, which is zipped again.
    if identity.is_some() && notarize(&zip, &app)? {
        ditto_zip(&app, &zip)?;
    }
    let mut out = vec![zip];
    let dmg = dist.join(format!("{base}.dmg"));
    let _ = std::fs::remove_file(&dmg);
    // The disk image holds the app and a link to /Applications to drag it onto.
    let stage = dist.join("dmg");
    let _ = std::fs::remove_dir_all(&stage);
    let mut c = Command::new("ditto");
    c.arg(&app).arg(stage.join("Guitario.app"));
    sh(c, "ditto (disk image)")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("/Applications", stage.join("Applications")).map_err(|e| format!("Applications link: {e}"))?;
    let mut c = Command::new("hdiutil");
    c.args(["create", "-volname", "Guitario", "-fs", "HFS+", "-format", "UDZO", "-srcfolder"]).arg(&stage).arg(&dmg);
    let made = sh(c, "hdiutil");
    let _ = std::fs::remove_dir_all(&stage);
    match made {
        Ok(()) => {
            if let Some(id) = &identity {
                codesign(id, &dmg, None)?;
                notarize(&dmg, &dmg)?;
            }
            out.push(dmg);
        }
        Err(e) => eprintln!("==> no .dmg: {e}"),
    }
    Ok(out)
}

/// Windows SDK's signtool: on the PATH, or the newest in the SDK's folder.
fn signtool() -> Option<PathBuf> {
    if have("signtool") {
        return Some(PathBuf::from("signtool"));
    }
    let kits = PathBuf::from(std::env::var_os("ProgramFiles(x86)")?).join("Windows Kits/10/bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(kits).ok()?.flatten().map(|e| e.path().join("x64/signtool.exe")).filter(|p| p.exists()).collect();
    versions.sort();
    versions.pop()
}

/// Signs with signtool when `GUITARIO_WINDOWS_CERT` names a .pfx file (its password in
/// `GUITARIO_WINDOWS_CERT_PASSWORD`) or `GUITARIO_WINDOWS_CERT_SHA1` the thumbprint of a
/// certificate in the store (a hardware token's); false when neither is set.
fn sign_windows(files: &[PathBuf]) -> Result<bool, String> {
    let (pfx, sha1) = (env("GUITARIO_WINDOWS_CERT"), env("GUITARIO_WINDOWS_CERT_SHA1"));
    if pfx.is_none() && sha1.is_none() {
        return Ok(false);
    }
    let tool = signtool().ok_or("signtool not found: install the Windows SDK")?;
    let mut c = Command::new(tool);
    c.args(["sign", "/fd", "SHA256", "/tr", "http://timestamp.digicert.com", "/td", "SHA256", "/d", "Guitario"]);
    match (pfx, sha1) {
        (Some(pfx), _) => {
            c.arg("/f").arg(pfx);
            if let Some(pw) = env("GUITARIO_WINDOWS_CERT_PASSWORD") {
                c.args(["/p", &pw]);
            }
        }
        (None, Some(sha1)) => {
            c.args(["/sha1", &sha1]);
        }
        (None, None) => return Ok(false),
    }
    c.args(files);
    sh(c, "signtool")?;
    Ok(true)
}

/// Inno Setup's compiler: on the PATH, or where its installer puts it.
fn iscc() -> Option<PathBuf> {
    if have("iscc") {
        return Some(PathBuf::from("iscc"));
    }
    ["ProgramFiles(x86)", "ProgramFiles"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|p| PathBuf::from(p).join("Inno Setup 6").join("ISCC.exe"))
        .find(|p| p.exists())
}

/// A folder zipped, and the Inno Setup installer when its compiler is installed; both signed
/// when a certificate is set (`sign_windows`).
fn package_windows(root: &Path, bins: &Path, dist: &Path, version: &str, arch: &str) -> Result<Vec<PathBuf>, String> {
    let name = format!("guitario-{version}-windows-{arch}");
    let dir = dist.join(&name);
    let _ = std::fs::remove_dir_all(&dir);
    let exes = [dir.join("guitario.exe"), dir.join("guitario-cli.exe")];
    for exe in &exes {
        copy(&bins.join(exe.file_name().unwrap_or_default()), exe)?;
    }
    let signed = sign_windows(&exes)?;
    if !signed {
        eprintln!("==> no certificate set: the programs are not signed");
    }
    for f in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "README.md"] {
        copy(&root.join(f), &dir.join(f))?;
    }
    // Windows' own tar is bsdtar, which writes zip files by the extension; Git's tar, which may be
    // first on the PATH, cannot.
    let zip = dist.join(format!("{name}.zip"));
    let _ = std::fs::remove_file(&zip);
    let tar = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from("tar"), |r| PathBuf::from(r).join("System32").join("tar.exe"));
    let mut c = Command::new(tar);
    c.arg("-a").arg("-cf").arg(&zip).arg("-C").arg(dist).arg(&name);
    sh(c, "tar (zip)")?;
    let mut out = vec![zip];
    match iscc() {
        Some(iscc) => {
            let mut c = Command::new(iscc);
            c.arg(format!("/DVersion={version}")).arg(format!("/DSource={}", dir.display())).arg(format!("/DOutput={}", dist.display()));
            c.arg(root.join("packaging/windows/guitario.iss"));
            sh(c, "Inno Setup")?;
            let setup = dist.join(format!("Guitario-{version}-windows-{arch}-setup.exe"));
            if signed {
                sign_windows(std::slice::from_ref(&setup))?;
            }
            out.push(setup);
        }
        None => eprintln!("==> Inno Setup not found: no installer"),
    }
    Ok(out)
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let mut target: Option<String> = None;
    let mut build = true;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "--target" => target = Some(it.next().ok_or(USAGE)?.to_string()),
            "--no-build" => build = false,
            _ => return Err(USAGE.into()),
        }
    }
    let os = match &target {
        Some(t) => Os::of(t).ok_or_else(|| format!("cannot package for `{t}`"))?,
        None => Os::host(),
    };
    let arch = target.as_deref().and_then(|t| t.split('-').next()).unwrap_or(std::env::consts::ARCH).to_string();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| e.to_string())?;
    let version = crate::version::read(&manifest)?;
    if build {
        let mut c = crate::cargo();
        c.args(["build", "--release", "--locked", "-p", "guitario", "-p", "guitario-cli"]);
        if let Some(t) = &target {
            c.args(["--target", t]);
        }
        sh(c, "cargo build --release")?;
    }
    let target_dir = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), |d| root.join(d));
    let bins = match &target {
        Some(t) => target_dir.join(t).join("release"),
        None => target_dir.join("release"),
    };
    for b in ["guitario", "guitario-cli"] {
        if !bins.join(os.exe(b)).exists() {
            return Err(format!("{} is missing: build first", bins.join(os.exe(b)).display()));
        }
    }
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist).map_err(|e| e.to_string())?;
    let made = match os {
        Os::Linux => package_linux(root, &bins, &dist, &version, &arch)?,
        Os::Mac => package_macos(root, &bins, &dist, &version, &arch)?,
        Os::Windows => package_windows(root, &bins, &dist, &version, &arch)?,
    };
    for f in made {
        eprintln!("==> {}", f.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_and_names() {
        assert_eq!(Os::of("aarch64-apple-darwin"), Some(Os::Mac));
        assert_eq!(Os::of("x86_64-pc-windows-msvc"), Some(Os::Windows));
        assert_eq!(Os::of("x86_64-unknown-linux-gnu"), Some(Os::Linux));
        assert_eq!(Os::of("wasm32-unknown-unknown"), None);
        assert_eq!((deb_arch("x86_64"), deb_arch("aarch64"), deb_arch("riscv64")), ("amd64", "arm64", "riscv64"));
        assert_eq!(Os::Windows.exe("guitario"), "guitario.exe");
        assert_eq!((civil(0), civil(20_744), civil(11_016)), ("1970-01-01".into(), "2026-10-18".into(), "2000-02-29".into()));
    }

    #[test]
    fn bad_arguments_are_refused_before_anything_is_built() {
        let root = crate::root();
        for args in [&["--target"][..], &["--bogus"], &["--target", "wasm32-unknown-unknown", "--no-build"]] {
            assert!(run(&root, args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn the_templates_name_every_file_type_the_app_opens() {
        let root = crate::root();
        let plist = template(&root, "macos/Info.plist", "1.2.3-rc.1").unwrap();
        assert!(!plist.contains('@') && plist.contains("<string>1.2.3</string>") && !plist.contains("rc.1"), "numbers only");
        let meta = template(&root, "linux/ai.storyteller.guitario.metainfo.xml", "1.2.3").unwrap();
        assert!(meta.contains("<release version=\"1.2.3\" date=\"") && !meta.contains('@'), "{meta}");
        assert_eq!(plist.matches("<dict>").count(), plist.matches("</dict>").count());
        assert_eq!(plist.matches("<array>").count(), plist.matches("</array>").count());
        for ext in ["gt", "gp", "gpx", "gp5", "gp4", "gp3", "musicxml", "mxl"] {
            assert!(plist.contains(&format!("<string>{ext}</string>")), "{ext}");
        }
        assert!(plist.contains("public.midi-audio") && plist.contains("NSMicrophoneUsageDescription"));
        let desktop = std::fs::read_to_string(root.join("packaging/linux/ai.storyteller.guitario.desktop")).unwrap();
        let mime = std::fs::read_to_string(root.join("packaging/linux/guitario-mime.xml")).unwrap();
        for ty in ["application/x-guitario", "application/x-guitar-pro"] {
            assert!(desktop.contains(ty) && mime.contains(ty), "{ty}");
        }
        for glob in ["*.gt", "*.gp", "*.gpx", "*.gp5", "*.gp4", "*.gp3"] {
            assert!(mime.contains(&format!("pattern=\"{glob}\"")), "{glob}");
        }
        assert!(desktop.contains("Exec=guitario %f") && desktop.contains("Icon=ai.storyteller.guitario"));
        let iss = std::fs::read_to_string(root.join("packaging/windows/guitario.iss")).unwrap();
        for ext in [".gt", ".gp", ".gp5", ".mid", ".musicxml"] {
            assert!(iss.contains(&format!("Classes\\{ext}")), "{ext}");
        }
        let control = deb_control("1.2.3", "x86_64", 4096);
        assert!(control.contains("Version: 1.2.3\n") && control.contains("Architecture: amd64\n") && control.ends_with('\n'));
        assert!(control.lines().filter(|l| l.starts_with(' ')).all(|l| l.len() < 80), "description lines are indented and short");
    }
}
