#![allow(dead_code)]
#[path="../../../../crates/ferrite/src/line.rs"]
mod line;
#[path="../../../../crates/ferrite/src/keymap.rs"]
mod keymap;
#[path="../../../../crates/ferrite-core/src/prompt_files.rs"]
mod prompt_files;
fn main() {
  let mut line = line::Line::default();
  line.replace(None,"Keep instruction one\nRevise instruction two\nKeep instruction three");
  line.place_caret("Keep instruction one\nRevise".len());
  line.delete_to_start();
  println!("Cmd+Backspace in second line => {:?}", line.text());
  for (key,action,context) in keymap::bindings(keymap::Platform::Windows).iter().filter(|(k,_,_)|k=="ctrl-a"||k=="ctrl-w") {
    println!("Windows {key}: {action} @ {context:?}");
  }
  let cwd=std::path::Path::new("/tmp/ferrite-example");
  let picked="@docs/Design Notes.md ";
  println!("Menu insertion {picked:?} resolves to {:?}",prompt_files::paths(picked,Some(cwd)));
  println!("Quoted alternative resolves to {:?}",prompt_files::paths("@\"docs/Design Notes.md\" ",Some(cwd)));
}
