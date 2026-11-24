use std::collections::BTreeMap;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-placement-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A Machine's registry of a Project at each of `roots`, made under `dir`.
fn registry(dir: &Path, roots: &[&str]) -> (Registry, Vec<(ProjectId, PathBuf)>) {
    let mut registry = Registry::open(&dir.join("registry")).unwrap();
    let projects = roots
        .iter()
        .map(|root| {
            let root = dir.join(root);
            fs::create_dir_all(&root).unwrap();
            let id = registry.register(&root).unwrap();
            (id, registry.project(id).unwrap().root.clone())
        })
        .collect();
    (registry, projects)
}

#[test]
fn a_counterpart_is_the_first_project_there_of_the_same_repository() {
    let dir = scratch("by-repository");
    let (there, projects) = registry(&dir, &["other", "app", "app-again"]);
    let repositories: Repositories = BTreeMap::from([
        (projects[0].0, "github.com/acme/other".to_string()),
        (projects[1].0, "github.com/acme/app".to_string()),
        (projects[2].0, "github.com/acme/app".to_string()),
    ]);
    let found = counterpart(
        Path::new("C:/src/app"),
        Some("github.com/acme/app"),
        Across::ToMachine(&[]),
        &there,
        &repositories,
    );
    assert_eq!(found, Some(projects[1].0));
}

#[test]
fn a_project_with_no_repository_is_found_by_its_placement_either_way() {
    let dir = scratch("by-placement");
    let (there, projects) = registry(&dir, &["notes", "elsewhere"]);
    let local = PathBuf::from("C:/Users/me/notes");
    let placements = [Placement {
        local_root: local.clone(),
        remote_root: projects[0].1.clone(),
    }];
    let none = Repositories::new();
    assert_eq!(
        counterpart(&local, None, Across::ToMachine(&placements), &there, &none),
        Some(projects[0].0)
    );

    let (here, local_projects) = registry(&dir.join("here"), &["notes"]);
    let back = [Placement {
        local_root: local_projects[0].1.clone(),
        remote_root: PathBuf::from("/home/me/projects/notes"),
    }];
    assert_eq!(
        counterpart(
            Path::new("/home/me/projects/notes"),
            None,
            Across::FromMachine(&back),
            &here,
            &none
        ),
        Some(local_projects[0].0)
    );
}

#[test]
fn a_project_with_neither_has_no_counterpart() {
    let dir = scratch("none");
    let (there, projects) = registry(&dir, &["app"]);
    let repositories: Repositories = BTreeMap::from([(projects[0].0, "github.com/acme/app".to_string())]);
    let root = Path::new("C:/src/tool");
    assert_eq!(
        counterpart(root, Some("github.com/acme/tool"), Across::ToMachine(&[]), &there, &repositories),
        None
    );
    assert_eq!(counterpart(root, None, Across::Between, &there, &repositories), None);
    let elsewhere = [Placement {
        local_root: PathBuf::from("C:/src/other"),
        remote_root: projects[0].1.clone(),
    }];
    assert_eq!(
        counterpart(root, None, Across::ToMachine(&elsewhere), &there, &repositories),
        None
    );
}

#[test]
fn a_placement_is_kept_once_per_project_of_this_computer() {
    let dir = scratch("kept");
    assert_eq!(placements(&dir), []);
    let placed = |remote: &str| Placement {
        local_root: "C:/src/notes".into(),
        remote_root: remote.into(),
    };
    place(&dir, placed("/home/me/notes")).unwrap();
    place(&dir, placed("/home/me/projects/notes")).unwrap();
    assert_eq!(placements(&dir), [placed("/home/me/projects/notes")]);
}

#[test]
fn a_destination_is_a_folder_under_the_home_or_the_root_never_either_itself() {
    for fine in ["~/projects/app", "/srv/app", "~/a b/c/", r"D:\work\app", "c:/work/app"] {
        assert_eq!(refuse_destination(fine), None, "{fine}");
    }
    for refused in ["", "~", "~/", "/", "projects/app", "~/../x", "/srv/../etc", r"D:\", "D:", r"D:\work\..\x"] {
        assert!(refuse_destination(refused).is_some(), "{refused:?}");
    }
}

#[test]
fn what_git_ignores_is_listed_each_directory_whole_with_its_size() {
    let root = scratch("ignored");
    crate::workspace::git_for_tests(&root, &["init", "-q"]);
    fs::write(root.join(".gitignore"), "target/\n*.log\n.env\n").unwrap();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::write(root.join("target/debug/app"), [0u8; 300]).unwrap();
    fs::write(root.join("target/notes.txt"), "built").unwrap();
    fs::write(root.join("run.log"), "log").unwrap();
    fs::write(root.join(".env"), "KEY=1\n").unwrap();

    let mut listed = ignored(&root);
    listed.sort_by(|a, b| a.path.cmp(&b.path));
    let entry = |path: &str, directory| Ignored {
        path: path.into(),
        directory,
    };
    assert_eq!(listed, [entry(".env", false), entry("run.log", false), entry("target", true)]);
    assert_eq!(size(&root, "target"), 305);
    assert_eq!(ignored(&scratch("not-git")), []);
}
