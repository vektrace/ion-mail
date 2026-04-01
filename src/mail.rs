pub fn send(to: Option<Vec<String>>, subject: Option<String>, body: Option<String>, attachments: Option<Vec<std::path::PathBuf>>, yes: bool) {
    // also have to do like all checks and if only some are done still open interactively
    todo!("Implement sending");
}

pub fn list(limit: usize, unread: bool, json: bool) {
    todo!("Implement listing the last {} elements", limit);
}

pub fn read(id: u32) {
    todo!("Implement reading mail with index {}", id);
}

pub fn search(query: String, folder: String, since: Option<String>) {
    // validate date too
    todo!("Implement searching for {} in folder {}", query, folder);
}

// mv because move is reserved by rust
pub fn mv(id: u32, from: String, to: String) {
    todo!("Implement moving mail with index {} from folder {} to folder {}", id, from, to);
}

pub fn draft(to: Option<Vec<String>>, subject: Option<String>, body: Option<String>, attachments: Option<Vec<std::path::PathBuf>>) {
    todo!("Implement drafts");
}
