use crate::{Config, account::auth, mail::read};

use chrono::{DateTime, Local};
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use std::process;

pub fn list(config: Config, stats: bool) {
    let mut imap_session = auth(config);

    let mailboxes = imap_session.list(None, Some("*")).unwrap();

    let mut msgs: Vec<(u32, String)> = Vec::new();

    for mailbox in mailboxes.iter() {
        if stats {
            if !mailbox
                .attributes()
                .contains(&imap::types::NameAttribute::NoSelect)
            {
                let msg_cnt = imap_session.examine(mailbox.name()).expect(&format!(
                    "Failed to retrieve total messages for folder {}",
                    mailbox.name()
                ));
                msgs.push((msg_cnt.exists, mailbox.name().to_string()))
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

    imap_session.logout().unwrap();
}

pub fn view(config: Config, folder: String, page_size: usize) {
    let mut imap_session = auth(config.clone());

    match imap_session.select(&folder) {
        Ok(_) => {}
        Err(_) => {
            eprintln!("Failed to select folder");
            process::exit(1);
        }
    }

    let mails = imap_session.fetch("1:*", "(BODY.PEEK[HEADER])").unwrap();

    let mut current_id = 0;

    let mut messages: Vec<String> = Vec::new();

    for mail in mails.iter().rev() {
        if let Some(header_bytes) = mail.header() {
            let (parsed_headers, _) = mailparse::parse_headers(header_bytes).unwrap();

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

            let parsed_date = DateTime::parse_from_rfc2822(&date)
                .unwrap()
                .with_timezone(&Local);

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

    let _ = imap_session.logout();

    if !messages.is_empty() {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .default(0)
            .items(&messages)
            .clear(false)
            .max_length(page_size)
            .interact_opt()
            .unwrap();

        if let Some(selection) = selection {
            read(config, folder, selection.try_into().unwrap());
        }
    } else {
        println!("Folder {} is empty", folder);
    }
}

pub fn create(config: Config, name: String, parents: bool) {
    let mut imap_session = auth(config);

    let mailboxes = imap_session.list(None, Some("*")).unwrap();

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
                let mailboxes = imap_session.list(None, Some("*")).unwrap();

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
                    {
                        if let Err(_) = imap_session.delete(&mailbox.name()) {
                            eprintln!("Failed to delete folder {}", mailbox.name());
                        }
                    }
                }
                println!("Successfully deleted folder {}", name);
            } else {
                if let Err(_) = imap_session.delete(&name) {
                    eprintln!("Failed to delete folder {}", name);
                } else {
                    println!("Successfully deleted folder {}", name);
                }
            }
        }
    }

    let _ = imap_session.logout();
}

pub fn empty(config: Config, name: String) {
    let mut imap_session = auth(config);

    match imap_session.select(&name) {
        Ok(_) => {}
        Err(_) => {
            eprintln!("Failed to select folder");
            process::exit(1);
        }
    }

    let search_results = imap_session.uid_search("ALL").unwrap();

    if !search_results.is_empty() {
        let uid_list: Vec<String> = search_results.iter().map(|id| id.to_string()).collect();
        let uid_set = uid_list.join(",");

        imap_session
            .uid_store(uid_set, "+FLAGS.SILENT (\\Deleted)")
            .expect("Emptying failed");
        imap_session.expunge().expect("Emptying failed");

        println!("Folder {} emptied", name);
    } else {
        println!("Folder {} is already empty", name);
    }

    let _ = imap_session.noop();

    let _ = imap_session.logout();
}
