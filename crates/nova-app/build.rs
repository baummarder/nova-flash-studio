//! Embeds the Windows icon and version resource into the executable.
//!
//! The resource compiler is part of the Windows SDK, not of Rust. We only run
//! it for Windows targets and fail softly when it is unavailable so that
//! contributors without the SDK can still build and test the workspace. Release
//! packaging checks that the resulting executable really contains version data.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NOVA_EDITION");
    println!("cargo:rerun-if-changed=../../assets/icons/nova-flash-studio.ico");
    println!("cargo:rerun-if-changed=../../assets/branding/nova-logo.svg");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let icon = manifest.join("../../assets/icons/nova-flash-studio.ico");
    if !icon.is_file() {
        println!("cargo:warning=Windows icon not found at {}", icon.display());
        return;
    }

    let Some(rc) = find_rc() else {
        println!(
            "cargo:warning=rc.exe not found; building without embedded Windows icon/version resource"
        );
        return;
    };

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let script = out.join("nova-version.rc");
    let resource = out.join("nova-version.res");
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    let (major, minor, patch) = parse_version(&version);
    let icon_path = icon.to_string_lossy().replace('\\', "/");

    // Edition-aware version resource (M19): NOVA_EDITION is set by the
    // packaging scripts; anything else falls back to the User product name.
    let edition = env::var("NOVA_EDITION").unwrap_or_else(|_| "user".to_string());
    let product_name = match edition.trim().to_ascii_lowercase().as_str() {
        "edu" => "N\u{00D8}VA Flash Studio Edu",
        "enterprise" => "N\u{00D8}VA Flash Studio Enterprise",
        _ => "N\u{00D8}VA Flash Studio",
    };
    let internal_name = match edition.trim().to_ascii_lowercase().as_str() {
        "edu" => "nova-flash-studio-edu",
        "enterprise" => "nova-flash-studio-enterprise",
        _ => "nova-flash-studio",
    };
    let original_filename = format!("{product_name}.exe");

    // M29.5: publisher identity is configuration, not hardcoded branding.
    // NOVA_COMPANY_NAME / NOVA_COPYRIGHT are set by the release pipeline;
    // the defaults are documented placeholders pending rights-holder
    // confirmation (see docs/CODE_SIGNING.md). No invented company.
    let company = env::var("NOVA_COMPANY_NAME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "N\u{00D8}VA Flash Studio contributors".to_string());
    let copyright = env::var("NOVA_COPYRIGHT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "MIT OR Apache-2.0".to_string());

    let rc_text = format!(
        r#"#pragma code_page(65001)
1 ICON "{icon_path}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "{company}\0"
      VALUE "FileDescription", "{product_name}\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "InternalName", "{internal_name}\0"
      VALUE "LegalCopyright", "{copyright}\0"
      VALUE "OriginalFilename", "{original_filename}\0"
      VALUE "ProductName", "{product_name}\0"
      VALUE "ProductVersion", "{version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
    );
    if let Err(error) = fs::write(&script, rc_text) {
        println!("cargo:warning=could not write resource script: {error}");
        return;
    }

    let status = Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&script)
        .status();
    match status {
        Ok(status) if status.success() && resource.is_file() => {
            println!("cargo:rustc-link-arg={}", resource.display());
        }
        Ok(status) => println!(
            "cargo:warning=rc.exe failed with {status}; building without icon/version resource"
        ),
        Err(error) => println!("cargo:warning=could not run rc.exe: {error}"),
    }
}

fn parse_version(version: &str) -> (u16, u16, u16) {
    let mut parts = version
        .split(['.', '-', '+'])
        .map(|part| part.parse::<u16>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn find_rc() -> Option<PathBuf> {
    if let Ok(path) = env::var("RC") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        _ => "x64",
    };
    let kits = env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .or_else(|| env::var_os("ProgramFiles").map(PathBuf::from))?
        .join("Windows Kits")
        .join("10")
        .join("bin");
    let mut versions: Vec<PathBuf> = fs::read_dir(&kits)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    versions.sort();
    versions.reverse();
    for version in versions {
        let candidate = version.join(arch).join("rc.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
