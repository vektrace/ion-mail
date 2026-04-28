use crate::{APP_NAME, Account, Config, account::auth};

use chrono::{DateTime, Local};
use dialoguer::{Confirm, Editor, Input, MultiSelect, Select, theme::ColorfulTheme};
use keyring::Entry;
use mailparse::{DispositionType, MailHeaderMap, ParsedMail};
use minus::Pager;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process;
use termimad::MadSkin;

pub fn send(
    config: Config,
    to: Option<Vec<String>>,
    subject: Option<String>,
    body: Option<String>,
    attachments: Option<Vec<std::path::PathBuf>>,
    yes: bool,
) {
    // also have to do like all checks and if only some are done still open interactively
    todo!("Implement sending");
}

fn find_plain_text(part: &ParsedMail) -> Option<String> {
    if part.ctype.mimetype == "text/plain" {
        return part.get_body().ok();
    }

    for subpart in &part.subparts {
        if let Some(text) = find_plain_text(subpart) {
            return Some(text);
        }
    }

    None
}

fn find_html_text(part: &ParsedMail) -> Option<String> {
    if part.ctype.mimetype == "text/html" {
        return part.get_body().ok();
    }

    for subpart in &part.subparts {
        if let Some(text) = find_html_text(subpart) {
            return Some(text);
        }
    }

    None
}

pub fn read(config: Config, folder: String, id: u32) {
    let mut imap_session = auth(config);

    match imap_session.select(folder) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to select folder: {}", e);
            process::exit(1);
        }
    }

    let mails = match imap_session.fetch("1:*", "(FLAGS)") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            process::exit(1);
        }
    };

    let mut current_id = 0;
    let mut found = false;

    for _mail in mails.iter().rev() {
        if current_id == id {
            found = true;
        }
        current_id += 1;
    }

    if !found {
        eprintln!("Mail with ID {} could not be found", id);
        process::exit(1);
    }

    let max_value = mails.len();

    let found_mail = match imap_session.fetch(format!("{}", max_value - (id as usize)), "(BODY[])")
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            process::exit(1);
        }
    };

    match imap_session.logout() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Logout failed, ignoring... ({})", e);
        }
    }

    let mail_bytes = found_mail[0].body();

    let parsed = match mailparse::parse_mail(mail_bytes.unwrap()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to parse mail: {}", e);
            process::exit(1);
        }
    };

    let subject = parsed
        .headers
        .get_first_value("Subject")
        .unwrap_or_default();
    let from = parsed.headers.get_first_value("From").unwrap_or_default();
    let to = parsed.headers.get_first_value("To").unwrap_or_default();
    let date = parsed.headers.get_first_value("Date").unwrap_or_default();
    let cc = parsed.headers.get_first_value("Cc").unwrap_or_default();

    let html_body = find_html_text(&parsed);
    let plain_body = find_plain_text(&parsed);

    let body = match (html_body, plain_body) {
        (Some(html), _) => {
            let md = html2md::parse_html(&html);
            let skin = MadSkin::default();
            skin.term_text(&md).to_string()
        }
        (None, Some(plain)) => plain,
        (None, None) => " (No content) ".to_string(),
    };

    let parsed_date = match DateTime::parse_from_rfc2822(&date) {
        Ok(d) => d.with_timezone(&Local),
        Err(e) => {
            eprintln!("Failed to parse date: {}", e);
            process::exit(1);
        }
    };

    let mut attachments: Vec<String> = Vec::new();

    let mut current_attachment_id = 0;

    for part in parsed.subparts {
        let disposition = part.get_content_disposition();

        if disposition.disposition == DispositionType::Attachment
            || disposition.params.contains_key("filename")
        {
            let name = disposition
                .params
                .get("filename")
                .cloned()
                .unwrap_or_else(|| "unnamed_file".to_string());

            attachments.push(format!("[{:03}] {}", current_attachment_id, name));

            current_attachment_id += 1;
        }
    }

    let mut content: String = "".to_string();

    content.push_str(&format!("From:     {}", from));
    content.push_str(&format!("\nTo:       {}", to));
    if !cc.is_empty() {
        content.push_str(&format!("\nCc:    {}", cc));
    }
    content.push_str(&format!(
        "\nDate:     {}",
        parsed_date.format("%Y-%m-%d %I:%M:%S %p")
    ));
    content.push_str(&format!("\nSubject:  {}", subject));
    if !attachments.is_empty() {
        content.push_str(&format!("\nAttachments:  {}", attachments.join(", ")));
    }
    content.push_str(&format!("\n\n{}", body));

    let pager = Pager::new();
    match pager.set_prompt("Reading email (Press 'q' to exit)") {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            process::exit(1);
        }
    }
    match pager.set_text(content) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            process::exit(1);
        }
    }

    match minus::dynamic_paging(pager) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            process::exit(1);
        }
    }
}

pub fn download(
    config: Config,
    folder: String,
    id: u32,
    attachment_id: Option<Vec<u32>>,
    save_folder: String,
) {
    let mut imap_session = auth(config);

    match imap_session.select(folder) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to select folder: {}", e);
            process::exit(1);
        }
    }

    let mails = match imap_session.fetch("1:*", "(FLAGS)") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Unexpected Error: {}", e);
            process::exit(1);
        }
    };

    let mut current_id = 0;
    let mut found = false;

    for _mail in mails.iter().rev() {
        if current_id == id {
            found = true;
        }
        current_id += 1;
    }

    if !found {
        eprintln!("Mail with ID {} could not be found", id);
        process::exit(1);
    }

    let max_value = mails.len();

    let found_mail =
        match imap_session.fetch(format!("{}", max_value - (id as usize)), "(BODY.PEEK[])") {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Unexpected Error: {}", e);
                process::exit(1);
            }
        };

    let mail_bytes = found_mail[0].body();

    let parsed = match mailparse::parse_mail(mail_bytes.unwrap()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to parse mail: {}", e);
            process::exit(1);
        }
    };

    let mut current_attachment_id = 0;

    for part in parsed.subparts {
        let disposition = part.get_content_disposition();

        if disposition.disposition == DispositionType::Attachment
            || disposition.params.contains_key("filename")
        {
            let filename = disposition
                .params
                .get("filename")
                .cloned()
                .unwrap_or_else(|| format!("attachment_{}.bin", current_attachment_id));

            let clean_filename = std::path::Path::new(&filename)
                .file_name()
                .ok_or("Invalid Filename")
                .unwrap();

            if let Some(ref id) = attachment_id {
                if id.contains(&current_attachment_id) {
                    let mut save_path = PathBuf::from(save_folder.clone());
                    save_path.push(clean_filename);

                    let binary_data = part.get_body_raw().unwrap();

                    let mut file = File::create(&save_path).unwrap();
                    match file.write_all(&binary_data) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Failed to save file: {}", e);
                            process::exit(1);
                        }
                    }
                }
            } else {
                let mut save_path = PathBuf::from(save_folder.clone());
                save_path.push(clean_filename);

                let binary_data = part.get_body_raw().unwrap();

                let mut file = File::create(&save_path).unwrap();
                match file.write_all(&binary_data) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Failed to save file: {}", e);
                        process::exit(1);
                    }
                }
            }
            current_attachment_id += 1;
        }
    }

    if let Some(id) = attachment_id {
        println!(
            "Successfully saved attachment(s) {} to {}",
            id.iter()
                .map(|n| n.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            save_folder
        );
    } else {
        println!("Successfully saved all attachments to {}", save_folder);
    }
}

pub fn search(config: Config, query: String, folder: String) {
    let mut imap_session = auth(config.clone());

    let mut results: Vec<(String, String)> = Vec::new();

    if folder != "ALL" {
        match imap_session.select(&folder) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to select folder: {}", e);
                process::exit(1);
            }
        }

        let fetch = match imap_session.fetch("1:*", "(BODY.PEEK[])") {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Unexpected Error: {}", e);
                process::exit(1);
            }
        };

        let mut current_id = 0;

        for mail in fetch.iter().rev() {
            let parsed = match mailparse::parse_mail(mail.body().unwrap()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to parse mail: {}", e);
                    process::exit(1);
                }
            };

            let subject = parsed
                .headers
                .get_first_value("Subject")
                .unwrap_or_else(|| "".to_string());
            let from = parsed
                .headers
                .get_first_value("From")
                .unwrap_or_else(|| "".to_string());
            let body = find_plain_text(&parsed).unwrap_or_else(|| "".to_string());

            if subject.contains(&query) || from.contains(&query) || body.contains(&query) {
                results.push((
                    format!("[{:03}] | {} | {}", current_id, subject, from),
                    folder.clone(),
                ));
            }
            current_id += 1;
        }
    } else {
        let mailboxes = match imap_session.list(None, Some("*")) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Unexpected Error: {}", e);
                process::exit(1);
            }
        };

        for mailbox in mailboxes.iter() {
            match imap_session.select(&mailbox.name()) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Failed to select folder: {}", e);
                    process::exit(1);
                }
            }

            let fetch = match imap_session.fetch("1:*", "(BODY.PEEK[])") {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                    process::exit(1);
                }
            };

            let mut current_id = 0;

            for mail in fetch.iter().rev() {
                let parsed = match mailparse::parse_mail(mail.body().unwrap()) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Failed to parse mail: {}", e);
                        process::exit(1);
                    }
                };

                let subject = parsed
                    .headers
                    .get_first_value("Subject")
                    .unwrap_or_else(|| "".to_string());
                let from = parsed
                    .headers
                    .get_first_value("From")
                    .unwrap_or_else(|| "".to_string());
                let body = find_plain_text(&parsed).unwrap_or_else(|| "".to_string());

                if subject.contains(&query) || from.contains(&query) || body.contains(&query) {
                    results.push((
                        format!(
                            "[{:03}] | {} | {} | {}",
                            current_id,
                            mailbox.name(),
                            subject,
                            from
                        ),
                        mailbox.name().to_string(),
                    ));
                }
                current_id += 1;
            }
        }
    }

    let selection = Select::with_theme(&ColorfulTheme::default())
        .default(0)
        .items(&results.iter().map(|n| n.0.clone()).collect::<Vec<String>>())
        .clear(false)
        .max_length(20)
        .interact_opt()
        .unwrap();

    if let Some(selection) = selection {
        let s = match selection.try_into() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Unexpected Error: {}", e);
                process::exit(1);
            }
        };
        read(config, results[s as usize].1.clone(), s);
    }

    match imap_session.logout() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Logout failed: {}", e);
            process::exit(1);
        }
    }
}

// mv because move is reserved by rust
pub fn mv(config: Config, from: String, to: String, id: Vec<u32>) {
    let mut imap_session = auth(config);

    if Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Are you sure you want to move mail?")
        .default(false)
        .show_default(true)
        .wait_for_newline(true)
        .interact()
        .unwrap()
    {
        match imap_session.select(&from) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to select folder: {}", e);
            }
        }

        let mails = match imap_session.fetch("1:*", "(FLAGS)") {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Unexpected Error: {}", e);
                process::exit(1);
            }
        };

        let mut current_id = 0;
        let mut found_ids = Vec::new();

        for _mail in mails.iter().rev() {
            if id.contains(&current_id) {
                found_ids.push(current_id);
            }
            current_id += 1;
        }

        if found_ids.len() != id.len() {
            eprintln!(
                "Mail with ID {} could not be found",
                id.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );
            process::exit(1);
        }

        let max_value = mails.len();

        for index in &id {
            match imap_session.mv(&format!("{}", max_value - (*index as usize)), &to) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                }
            };
        }

        println!(
            "Mail with ID {} has been moved from {} to {}",
            id.iter()
                .map(|n| n.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            from,
            to
        );
    }
}

pub fn delete(config: Config, id: Option<Vec<u32>>, folder: String) {
    let mut imap_session = auth(config);

    if Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Are you sure you want to delete mail?")
        .default(true)
        .show_default(true)
        .wait_for_newline(true)
        .interact()
        .unwrap()
    {
        match imap_session.select(&folder) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to select folder: {}", e);
                process::exit(1);
            }
        }

        if let Some(id) = id {
            let mails = match imap_session.fetch("1:*", "(UID)") {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                    process::exit(1);
                }
            };

            let mut current_id = 0;
            let mut found_ids = Vec::new();

            for mail in mails.iter().rev() {
                if id.contains(&current_id) {
                    if let Some(uid) = mail.uid {
                        found_ids.push(uid);
                    }
                }
                current_id += 1;
            }

            if found_ids.len() != id.len() {
                eprintln!(
                    "Mail with ID {} could not be found",
                    id.iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                );
                process::exit(1);
            }

            let uid_set = found_ids
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<String>>()
                .join(",");

            match imap_session.uid_store(&uid_set, "FLAGS (\\Deleted)") {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                }
            }

            match imap_session.uid_expunge(&uid_set) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                    process::exit(1);
                }
            }
        } else {
            let mails = match imap_session.fetch("1:*", "(UID, BODY.PEEK[HEADER])") {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                    process::exit(1);
                }
            };

            let mut current_id = 0;
            let mut messages: Vec<String> = Vec::new();

            for mail in mails.iter().rev() {
                if let Some(header_bytes) = mail.header() {
                    let (parsed_headers, _) = match mailparse::parse_headers(header_bytes) {
                        Ok(parsed) => parsed,
                        Err(e) => {
                            eprintln!("Failed to parse headers: {}", e);
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
                        Ok(d) => d.with_timezone(&Local),
                        Err(e) => {
                            eprintln!("Failed to parse date: {}", e);
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

            if !messages.is_empty() {
                let selection = MultiSelect::with_theme(&ColorfulTheme::default())
                    .items(&messages)
                    .max_length(20)
                    .interact_opt()
                    .unwrap();

                if let Some(selection) = selection {
                    let mut current_id = 0;
                    let mut found_ids = Vec::new();

                    for mail in mails.iter().rev() {
                        if selection.contains(&current_id) {
                            if let Some(uid) = mail.uid {
                                found_ids.push(uid);
                            }
                        }
                        current_id += 1;
                    }

                    if found_ids.len() != selection.len() {
                        // should not be possible
                        eprintln!(
                            "Mail with ID {} could not be found",
                            selection
                                .iter()
                                .map(|n| n.to_string())
                                .collect::<Vec<String>>()
                                .join(", ")
                        );
                        process::exit(1);
                    }

                    let uid_set = found_ids
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<String>>()
                        .join(",");

                    match imap_session.uid_store(&uid_set, "FLAGS (\\Deleted)") {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Unexpected Error: {}", e);
                            process::exit(1);
                        }
                    }

                    match imap_session.uid_expunge(&uid_set) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Unexpected Error: {}", e);
                            process::exit(1);
                        }
                    }
                }
            } else {
                println!("Folder {} is empty", folder);
            }
        }

        match imap_session.logout() {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Logout failed, ignoring... ({})", e);
            }
        }
    }
}

pub fn draft(
    config: Config,
    to: Option<Vec<String>>,
    subject: Option<String>,
    body: Option<String>,
    attachments: Option<Vec<std::path::PathBuf>>,
) {
    todo!("Implement drafts");
}
