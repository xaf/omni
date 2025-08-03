use std::process::Command as ProcessCommand;

use lazy_static::lazy_static;

lazy_static! {
    static ref RELEASE_ARCH: String = {
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            _ => std::env::consts::ARCH,
        };
        arch.to_string()
    };
    static ref RELEASE_OS: String = {
        let os = match std::env::consts::OS {
            "macos" => "darwin",
            _ => std::env::consts::OS,
        };
        os.to_string()
    };
    static ref ROSETTA_AVAILABLE: bool = compute_check_rosetta_available();
}

pub fn current_os() -> String {
    (*RELEASE_OS).clone()
}

pub fn current_arch() -> String {
    (*RELEASE_ARCH).clone()
}

const RELEASE_ARCH_X86_64: &[&str] = &["x86_64", "amd64", "x64"];
const RELEASE_ARCH_ARM64: &[&str] = &["arm64", "aarch64", "aarch_64"];
const RELEASE_ARCH_DARWIN_UNIVERSAL: &[&str] = &["universal", "all", "any"];

/// This function returns the compatible release architectures for the current
/// system, based on the current architecture. It returns a vector of vectors
/// as there are different layers of compatibility that should be followed for
/// preference, e.g. for Darwin, we will find direct-compatibility, universal
/// compatibility, and finally Rosetta compatibility.
pub fn compatible_release_arch() -> Vec<Vec<String>> {
    let mut archs = vec![];

    // First add the direct compatibility
    archs.push(if *RELEASE_ARCH == "x86_64" {
        RELEASE_ARCH_X86_64.iter().map(|s| s.to_string()).collect()
    } else if *RELEASE_ARCH == "arm64" {
        RELEASE_ARCH_ARM64.iter().map(|s| s.to_string()).collect()
    } else {
        vec![(*RELEASE_ARCH).to_string()]
    });

    // Then, if we're on Darwin, add the universal compatibility
    if *RELEASE_OS == "darwin" {
        archs.push(
            RELEASE_ARCH_DARWIN_UNIVERSAL
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );

        // Finally, if Rosetta is available, add the Rosetta compatibility
        if check_rosetta_available() {
            archs.push(RELEASE_ARCH_X86_64.iter().map(|s| s.to_string()).collect());
        }
    }

    archs
}

fn compute_check_rosetta_available() -> bool {
    if *RELEASE_OS != "darwin" || *RELEASE_ARCH == "x86_64" {
        return false;
    }

    // Verify that /usr/bin/pgrep, /usr/bin/arch and /usr/bin/uname
    // exist and are executable
    for binary in &["/usr/bin/pgrep", "/usr/bin/arch", "/usr/bin/uname"] {
        if !std::path::Path::new(binary).exists() || !std::path::Path::new(binary).is_file() {
            return false;
        }

        // Get the metadata
        let metadata = match std::fs::metadata(binary) {
            Ok(metadata) => metadata,
            Err(_) => return false,
        };

        // Check if it's executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return false;
            }
        }
    }

    // Verify that the `oahd` process is running; if not,
    // it means Rosetta is not available
    if !ProcessCommand::new("/usr/bin/pgrep")
        .arg("oahd")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        return false;
    }

    // Run `uname -m` through `arch` and check if the answer is `x86_64`, we also
    // redirect the error output to `/dev/null` as we don't care about it
    let output = ProcessCommand::new("/usr/bin/arch")
        .arg("-x86_64")
        .arg("/usr/bin/uname")
        .arg("-m")
        .stderr(std::process::Stdio::null())
        .output();

    // Validate that the output is `x86_64`
    output
        .map(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "x86_64"
        })
        .unwrap_or(false)
}

fn check_rosetta_available() -> bool {
    *ROSETTA_AVAILABLE
}

pub fn compatible_release_os() -> Vec<String> {
    if *RELEASE_OS == "darwin" {
        vec!["darwin".to_string(), "macos".to_string(), "osx".to_string()]
    } else {
        vec![(*RELEASE_OS).to_string()]
    }
}
