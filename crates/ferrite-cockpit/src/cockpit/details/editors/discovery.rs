//! Native executable discovery and launch, off the UI thread.

use super::catalog::DEFINITIONS;
use super::{EditorId, InstalledEditor};
use ferrite_core::spawn::NoConsoleWindow;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub(super) fn launch(editor: &InstalledEditor, path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Folder not found: {}", path.display()),
        ));
    }
    let program = editor.program.as_ref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Editor executable not found")
    })?;
    let mut child = Command::new(program)
        .args(editor.base_args)
        .arg(path)
        .env_remove("ELECTRON_RUN_AS_NODE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .no_console_window()
        .spawn()?;
    // Some IDE launchers become the GUI process. Reap it without holding
    // an executor worker or making a successful launch wait for IDE exit.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub(super) fn discover() -> Vec<InstalledEditor> {
    let path_dirs: Vec<_> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    DEFINITIONS
        .iter()
        .enumerate()
        .filter_map(|(index, definition)| {
            let id = EditorId(index);
            if id == EditorId::FILE_MANAGER {
                return (cfg!(target_os = "macos")
                    || cfg!(windows)
                    || resolve_command("xdg-open", &path_dirs).is_some())
                .then_some(InstalledEditor {
                    id,
                    program: None,
                    base_args: &[],
                });
            }
            let mut candidates = Vec::new();
            let prefer_bundle = cfg!(target_os = "macos") && id.id() == "zed";
            let prefer_zeditor = cfg!(target_os = "linux") && id.id() == "zed";
            if !prefer_bundle && !prefer_zeditor {
                for command in definition.commands {
                    candidates.extend(resolve_commands(command, &path_dirs));
                }
            }
            if cfg!(windows) && id.id() == "zed" {
                // A stable install can be behind Preview/Nightly in PATH.
                candidates.sort_by_key(|path| prerelease_zed(path));
            }
            let mut toolbox = None;
            if cfg!(target_os = "macos") {
                let roots = home
                    .iter()
                    .map(|home| home.join("Applications"))
                    .chain([PathBuf::from("/Applications")]);
                for root in roots {
                    let mut bundles: Vec<_> = definition
                        .names
                        .iter()
                        .map(|name| format!("{name}.app"))
                        .collect();
                    bundles.extend(directory_names(&root).into_iter().filter(|entry| {
                        definition
                            .names
                            .iter()
                            .any(|name| versioned_bundle(entry, name))
                    }));
                    for bundle in bundles {
                        let contents = root.join(bundle).join("Contents");
                        if id.jetbrains() || id.id() == "zed" {
                            candidates.push(contents.join("MacOS").join(if id.id() == "zed" {
                                "cli"
                            } else {
                                definition.commands[0]
                            }));
                        } else {
                            candidates.push(
                                contents
                                    .join("Resources/app/bin")
                                    .join(definition.commands[0]),
                            );
                            candidates.push(contents.join("Resources/app/bin/code"));
                        }
                    }
                }
                if id.jetbrains() {
                    toolbox = home.as_ref().map(|home| {
                        home.join("Library/Application Support/JetBrains/Toolbox/scripts")
                    });
                }
            } else if cfg!(windows) {
                let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
                let roots: Vec<_> = local
                    .iter()
                    .map(|path| path.join("Programs"))
                    .chain(
                        ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
                            .into_iter()
                            .filter_map(std::env::var_os)
                            .map(PathBuf::from),
                    )
                    .collect();
                if id.jetbrains() {
                    toolbox = local.map(|path| path.join("JetBrains/Toolbox/scripts"));
                    for root in roots
                        .iter()
                        .flat_map(|root| [root.clone(), root.join("JetBrains")])
                    {
                        for entry in directory_names(&root).into_iter().filter(|entry| {
                            definition.names.iter().any(|name| {
                                entry.as_str() == *name || entry.starts_with(&format!("{name} "))
                            })
                        }) {
                            for suffix in ["64.exe", ".exe"] {
                                candidates.push(
                                    root.join(&entry)
                                        .join("bin")
                                        .join(format!("{}{suffix}", definition.commands[0])),
                                );
                            }
                        }
                    }
                } else {
                    let name = match id.id() {
                        "vscode" => "Microsoft VS Code",
                        "vscode-insiders" => "Microsoft VS Code Insiders",
                        _ => definition.label,
                    };
                    for root in roots {
                        for bin in ["resources/app/bin", "bin"] {
                            candidates.push(
                                root.join(name)
                                    .join(bin)
                                    .join(format!("{}.cmd", definition.commands[0])),
                            );
                            candidates.push(root.join(name).join(bin).join("code.cmd"));
                        }
                        if id.id() == "zed" {
                            candidates.push(root.join(name).join("bin/zed.exe"));
                            candidates.push(root.join(name).join("zed.exe"));
                        }
                    }
                }
            } else {
                let mut dirs: Vec<_> = home
                    .iter()
                    .map(|home| home.join(".local/bin"))
                    .chain(["/usr/local/bin", "/usr/bin", "/snap/bin"].map(PathBuf::from))
                    .collect();
                if id.jetbrains() {
                    toolbox = std::env::var_os("XDG_DATA_HOME")
                        .map(PathBuf::from)
                        .or_else(|| home.as_ref().map(|home| home.join(".local/share")))
                        .map(|root| root.join("JetBrains/Toolbox/scripts"));
                    dirs.extend(toolbox.iter().cloned());
                }
                let commands = if prefer_zeditor {
                    &["zeditor", "zed"][..]
                } else {
                    definition.commands
                };
                for command in commands {
                    if prefer_zeditor {
                        candidates.extend(resolve_commands(command, &path_dirs));
                    }
                    candidates.extend(dirs.iter().map(|dir| dir.join(command)));
                }
            }
            if prefer_bundle {
                for command in definition.commands {
                    candidates.extend(resolve_commands(command, &path_dirs));
                }
            }
            if let Some(toolbox) = toolbox {
                let suffix = if cfg!(windows) { ".cmd" } else { "" };
                let command = definition.commands[0];
                candidates.push(toolbox.join(format!("{command}{suffix}")));
                let mut numbered: Vec<_> = directory_names(&toolbox)
                    .into_iter()
                    .filter(|entry| {
                        entry
                            .strip_prefix(command)
                            .and_then(|tail| tail.strip_suffix(suffix))
                            .is_some_and(|tail| {
                                !tail.is_empty() && tail.bytes().all(|byte| byte.is_ascii_digit())
                            })
                    })
                    .collect();
                numbered.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
                candidates.extend(numbered.into_iter().map(|name| toolbox.join(name)));
            }
            let program = candidates.into_iter().find(|candidate| {
                executable(candidate)
                    && (id.id() != "zed"
                        || cfg!(windows)
                        || candidate.file_name().is_none_or(|name| name != "zed")
                        || is_zed_editor(candidate))
            })?;
            let bundled = program.to_string_lossy().contains(".app/Contents/")
                || (cfg!(windows)
                    && !path_dirs
                        .iter()
                        .any(|dir| program.parent() == Some(dir.as_path())));
            let base_args = match id.id() {
                "cursor" => &["--classic"][..],
                "kiro" if !bundled => &["ide"][..],
                _ => &[],
            };
            Some(InstalledEditor {
                id,
                program: Some(program),
                base_args,
            })
        })
        .collect()
}

fn directory_names(path: &Path) -> Vec<String> {
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

fn prerelease_zed(path: &Path) -> bool {
    path.parent()
        .into_iter()
        .flat_map(|parent| parent.components())
        .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
        .filter(|part| part.contains("zed"))
        .last()
        .is_some_and(|part| part.contains("preview") || part.contains("nightly"))
}

fn versioned_bundle(entry: &str, name: &str) -> bool {
    entry
        .strip_prefix(name)
        .and_then(|tail| tail.strip_prefix(' '))
        .and_then(|tail| tail.strip_suffix(".app"))
        .is_some_and(|version| {
            version.starts_with(|ch: char| ch.is_ascii_digit())
                && version
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.')
        })
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn resolve_commands(command: &str, dirs: &[PathBuf]) -> Vec<PathBuf> {
    let suffixes = if cfg!(windows) {
        vec!["", ".exe", ".cmd", ".bat", ".com"]
    } else {
        vec![""]
    };
    dirs.iter()
        .flat_map(|dir| {
            suffixes
                .iter()
                .map(move |suffix| dir.join(format!("{command}{suffix}")))
        })
        .filter(|path| executable(path))
        .collect()
}

fn resolve_command(command: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    resolve_commands(command, dirs).into_iter().next()
}

fn is_zed_editor(program: &Path) -> bool {
    // OpenZFS also installs a `zed`. It must never appear as an editor.
    let Ok(mut child) = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return status.success()
                    && child
                        .wait_with_output()
                        .is_ok_and(|output| output.stdout.starts_with(b"Zed"))
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_channels_do_not_confuse_stable_and_insiders() {
        assert!(versioned_bundle(
            "IntelliJ IDEA 2026.1.app",
            "IntelliJ IDEA"
        ));
        assert!(!versioned_bundle(
            "Visual Studio Code - Insiders.app",
            "Visual Studio Code"
        ));
        assert!(!versioned_bundle("Zed Preview.app", "Zed"));
        assert!(!versioned_bundle("Cursor .app", "Cursor"));
    }

    #[test]
    fn stable_zed_is_not_mistaken_for_a_users_preview_named_home() {
        assert!(!prerelease_zed(Path::new(
            "/Users/zedpreview/Programs/Zed/bin/zed.exe"
        )));
        assert!(prerelease_zed(Path::new(
            "/Programs/Zed Preview/bin/zed.exe"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn discovery_requires_an_executable_and_launch_preserves_literal_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ferrite-editors-{unique}"));
        let folder = root.join("project with spaces and $(literal)");
        std::fs::create_dir_all(&folder).unwrap();
        let program = root.join("cursor");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$2/launch-args\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(resolve_command("cursor", std::slice::from_ref(&root)), None);
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            resolve_command("cursor", std::slice::from_ref(&root)),
            Some(program.clone())
        );
        launch(
            &InstalledEditor {
                id: EditorId(0),
                program: Some(program),
                base_args: &["--classic"],
            },
            &folder,
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let capture = folder.join("launch-args");
        let expected = format!("--classic\n{}\n", folder.display());
        while std::fs::read_to_string(&capture).ok().as_deref() != Some(expected.as_str())
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::read_to_string(capture).unwrap(), expected);
        assert!(launch(
            &InstalledEditor {
                id: EditorId(0),
                program: Some(root.join("removed")),
                base_args: &[]
            },
            &folder
        )
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
