pub fn add() {
    todo!("Implement logging in logic");
}

pub fn list() {
    todo!("Implement listing all users");
}

// not use because rust complained
pub fn switch(account: String) {
    todo!("Implement switching to account {}", account);
}

pub fn whoami() {
    todo!("Implement getting info about current user");
}

pub fn edit(account: Option<String>) {
    if let Some(account) = account {
        todo!("Implement editing account {}", account);
    } else {
        todo!("Implement editing current account");
    }
}

pub fn logout(account: Option<String>) {
    if let Some(account) = account {
        todo!("Implement logging out of account {}", account);
    } else {
        todo!("Implement logging out of current account");
    }
}
