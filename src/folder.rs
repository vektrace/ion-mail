use crate::{Config, account::auth, mail::read};

use chrono::{DateTime, Local};
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use std::process;

pub fn list(config: Config, stats: bool) {
    let mut imap_session = auth(config);

    let mailboxes = match imap_session.list(None, Some("*")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };

    let mut msgs: Vec<(u32, String)> = Vec::new();

    for mailbox in mailboxes.iter() {
        if stats {
            if !mailbox
                .attributes()
                .contains(&imap::types::NameAttribute::NoSelect)
            {
                let msg_cnt = match imap_session.examine(mailbox.name()) {
                    Ok(cnt) => cnt,
                    Err(e) => {
                        eprintln!(
                            "Failed to retrieve total messages for folder {}: {}",
                            mailbox.name(),
                            e
                        );
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                };
                msgs.push((msg_cnt.exists, mailbox.name().to_string()));
            }
        } else {
            msgs.push((0, mailbox.name().to_string()));
        }
    }

    for item in msgs {
        if stats {
            println!("{} [{}]", item.1, item.0)
        } else {
            println!("{}", item.1)
        }
    }

    match imap_session.logout() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Logout failed, ignoring... ({})", e);
        }
    };
}

pub fn view(config: Config, folder: String, page_size: usize) {
    let mut imap_session = auth(config.clone());

    match imap_session.select(&folder) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to select folder: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    }

    let mails = match imap_session.fetch("1:*", "(BODY.PEEK[HEADER])") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to fetch messages: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };

    let mut current_id = 0;

    let mut messages: Vec<String> = Vec::new();

    for mail in mails.iter().rev() {
        if let Some(header_bytes) = mail.header() {
            let (parsed_headers, _) = match mailparse::parse_headers(header_bytes) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to parse headers: {}", e);
                    keyring_core::unset_default_store();
                    process::exit(1);
                }
            };

            let mut subject = String::new();
            let mut from = String::new();
            let mut date = String::new();

            for header in parsed_headers {
                let key = header.get_key().to_lowercase();
                let value = header.get_value();

                match key.as_str() {
                    "subject" => subject = value,
                    "from" => from = value,
                    "date" => date = value,
                    _ => {}
                }
            }

            let parsed_date = match DateTime::parse_from_rfc2822(&date) {
                Ok(p) => p.with_timezone(&Local),
                Err(e) => {
                    eprintln!("Failed to parse date: {}", e);
                    keyring_core::unset_default_store();
                    process::exit(1);
                }
            };

            messages.push(format!(
                "[{:03}] {} | {} | {}",
                current_id,
                subject,
                from,
                parsed_date.format("%Y-%m-%d %I:%M:%S %p")
            ));

            current_id += 1;
        }
    }

    match imap_session.logout() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Logout failed, ignoring... ({})", e);
        }
    };

    if !messages.is_empty() {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .default(0)
            .items(&messages)
            .clear(false)
            .max_length(page_size)
            .interact_opt()
            .unwrap();

        if let Some(selection) = selection {
            let s = match selection.try_into() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                    keyring_core::unset_default_store();
                    process::exit(1);
                }
            };
            read(config, folder, s);
        }
    } else {
        println!("Folder {} is empty", folder);
    }
}

pub fn create(config: Config, name: String, parents: bool) {
    let mut imap_session = auth(config);

    let mailboxes = match imap_session.list(None, Some("*")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    };

    let delimiter = mailboxes[0].delimiter().unwrap();

    if parents {
        let folders = name.split(delimiter);

        let mut current_folder = String::new();

        for folder in folders {
            if !current_folder.is_empty() {
                current_folder.push_str(delimiter);
            }

            current_folder.push_str(folder);
            if imap_session.create(&current_folder).is_err() {
                eprintln!("Failed to create folder {}", current_folder);
            }
        }
        println!("Successfully created folder {}", name);
    } else {
        if imap_session.create(&name).is_err() {
            eprintln!("Failed to create folder {}", name);
        } else {
            println!("Successfully created folder {}", name);
        }
    }

    match imap_session.logout() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Logout failed, ignoring... ({})", e);
        }
    }
}

pub fn delete(config: Config, names: Vec<String>, recursive: bool, yes: bool) {
    let mut imap_session = auth(config);

    for name in names {
        let confirmation = if !yes {
            Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Are you sure you want to delete {}?", name))
                .default(false)
                .show_default(true)
                .wait_for_newline(true)
                .interact()
                .unwrap()
        } else {
            true
        };

        if confirmation {
            if recursive {
                let mailboxes = match imap_session.list(None, Some("*")) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Unexpected Error: {}", e);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                };

                let mut del_mailboxes = Vec::new();

                let search = format!("{}/", name);

                for mailbox in mailboxes.iter() {
                    if mailbox.name() == name {
                        del_mailboxes.push(mailbox);
                    }
                    if mailbox.name().starts_with(&search) {
                        del_mailboxes.push(mailbox);
                    }
                }

                del_mailboxes.sort_by_key(|k| k.name().to_string().len());
                del_mailboxes.reverse();

                for mailbox in del_mailboxes {
                    if !mailbox
                        .attributes()
                        .contains(&imap::types::NameAttribute::NoSelect)
                        && imap_session.delete(mailbox.name()).is_err()
                    {
                        eprintln!("Failed to delete folder {}", mailbox.name());
                    }
                }
                println!("Successfully deleted folder {}", name);
            } else {
                if imap_session.delete(&name).is_err() {
                    eprintln!("Failed to delete folder {}", name);
                } else {
                    println!("Successfully deleted folder {}", name);
                }
            }
        }
    }

    match imap_session.logout() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Logout failed, ignoring... ({})", e);
        }
    }
}

pub fn empty(config: Config, name: String) {
    if Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Are you sure you want to empty {}?", name))
        .default(false)
        .show_default(true)
        .wait_for_newline(true)
        .interact()
        .unwrap()
    {
        let mut imap_session = auth(config);

        match imap_session.select(&name) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to select folder: {}", e);
                keyring_core::unset_default_store();
                process::exit(1);
            }
        }

        let search_results = match imap_session.uid_search("ALL") {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Unexpected Error: {}", e);
                keyring_core::unset_default_store();
                process::exit(1);
            }
        };

        if !search_results.is_empty() {
            let uid_list: Vec<String> = search_results.iter().map(|id| id.to_string()).collect();
            let uid_set = uid_list.join(",");

            match imap_session.uid_store(&uid_set, "FLAGS (\\Deleted)") {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                    keyring_core::unset_default_store();
                    process::exit(1);
                }
            }

            // store doesnt work on some servers

            match imap_session.expunge() {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                    keyring_core::unset_default_store();
                    process::exit(1);
                }
            }

            println!("Folder {} emptied", name);
        } else {
            println!("Folder {} is already empty", name);
        }

        match imap_session.logout() {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Logout failed, ignoring... ({})", e);
            }
        }
    }
}
