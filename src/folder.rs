use crate::{APP_NAME, Config, Account};

use dialoguer::{theme::ColorfulTheme, Confirm, Select, MultiSelect};
use std::process;
use keyring::Entry;

pub fn list(config: Config, stats: bool) {
    let mut found = false;

    let mut use_account = Account {
        id: 0,
        active: false,
        email: "".to_string(),
        smtp: "".to_string(),
        smtp_port: 0,
        imap: "".to_string(),
        imap_port: 0,
    };

    for account in config.accounts {
        if account.active {
            found = true;
            use_account = account;
        }
    }
    
    if !found {
        println!("No account is currently active");
        process::exit(0);
    }

    let entry = Entry::new(APP_NAME, &use_account.id.to_string()).unwrap();
    let password = entry.get_password().unwrap();

    let tls = native_tls::TlsConnector::builder().build().unwrap();
    
    let client = if use_account.imap_port != 143 {
        imap::connect((use_account.imap.clone(), use_account.imap_port), &use_account.imap, &tls).unwrap()
    } else {
        imap::connect_starttls((use_account.imap.clone(), 143), &use_account.imap, &tls).unwrap()
    };

    let mut imap_session = client
        .login(use_account.email, password)
        .expect("Login failed");

    let mailboxes = imap_session.list(None, Some("*")).unwrap();

    let mut msgs: Vec<(u32, String)> = Vec::new();

    for mailbox in mailboxes.iter() {
        if stats {
            if !mailbox.attributes().contains(&imap::types::NameAttribute::NoSelect) {
                let msg_cnt = imap_session.examine(mailbox.name()).expect(&format!("Failed to retrieve total messages for folder {}", mailbox.name()));
                msgs.push((msg_cnt.exists, mailbox.name().to_string()))
            }
        } else {
            msgs.push((0, mailbox.name().to_string()));
        }
    }

    for item in msgs {
        if stats { println!("{} [{}]", item.1, item.0) }
        else { println!("{}", item.1) }
    }

    imap_session.logout().unwrap();
}

pub fn view(config: Config, folder: String, page_size: usize) {
    todo!("Implement viewing content of folder {} with {} mails on each page", folder, page_size);
}

pub fn create(config: Config, name: String, parents: bool) {
    let mut found = false;

    let mut use_account = Account {
        id: 0,
        active: false,
        email: "".to_string(),
        smtp: "".to_string(),
        smtp_port: 0,
        imap: "".to_string(),
        imap_port: 0,
    };

    for account in config.accounts {
        if account.active {
            found = true;
            use_account = account;
        }
    }

    if !found {
        println!("No account is currently active");
        process::exit(0);
    }

    let entry = Entry::new(APP_NAME, &use_account.id.to_string()).unwrap();
    let password = entry.get_password().unwrap();

    let tls = native_tls::TlsConnector::builder().build().unwrap();

    let client = if use_account.imap_port != 143 {
        imap::connect((use_account.imap.clone(), use_account.imap_port), &use_account.imap, &tls).unwrap()
    } else {
        imap::connect_starttls((use_account.imap.clone(), 143), &use_account.imap, &tls).unwrap()
    };

    let mut imap_session = client
        .login(use_account.email, password)
        .expect("Login failed");

    let mailboxes = imap_session
        .list(None, Some("*"))
        .unwrap();

    let delimiter = mailboxes[0]
        .delimiter()
        .expect("Failed to retrieve delimiter");

    if parents {
        let folders = name.split(delimiter);
        
        let mut current_folder = String::new();

        for folder in folders {
            if !current_folder.is_empty() {
                current_folder.push_str(delimiter);
            }

            current_folder.push_str(folder);
            if let Err(_) = imap_session.create(&current_folder) {
                eprintln!("Failed to create folder {}", current_folder);
            }
        }
        println!("Successfully created folder {}", name);
    } else {
        if let Err(_) = imap_session.create(&name) {
            eprintln!("Failed to create folder {}", name);
        } else {
            println!("Successfully created folder {}", name);
        }
    }

    let _ = imap_session.logout();
}

pub fn delete(config: Config, name: String, recursive: bool) {
    todo!("Implement deleting folder {}", name);
}

pub fn empty(config: Config, name: String) {
    todo!("Implement emptying folder {}", name);
}
