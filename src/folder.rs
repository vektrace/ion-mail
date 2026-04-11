pub fn list(toml_path: &str, stats: bool) {
    todo!("Implement listing all folders");
}

pub fn view(toml_path: &str, folder: String, page_size: usize) {
    todo!("Implement viewing content of folder {} with {} mails on each page", folder, page_size);
}

pub fn create(toml_path: &str, name: String, parents: bool) {
    todo!("Implement creating folder {}", name);
}

pub fn delete(toml_path: &str, name: String, recursive: bool) {
    todo!("Implement deleting folder {}", name);
}

pub fn empty(toml_path: &str, name: String) {
    todo!("Implement emptying folder {}", name);
}
