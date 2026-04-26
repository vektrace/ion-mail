use crate::{APP_NAME, Account, Config, account::auth};

use chrono::{DateTime, Local};
use dialoguer::{Confirm, Editor, Input, Select, theme::ColorfulTheme};
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
        Err(_) => {
            eprintln!("Failed to select folder");
            process::exit(1);
        }
    }

    let mails = imap_session.fetch("1:*", "(FLAGS)").unwrap();

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

    let found_mail = imap_session
        .fetch(format!("{}", max_value - (id as usize)), "(BODY[])")
        .unwrap();

    let mail_bytes = found_mail[0].body();

    let parsed = mailparse::parse_mail(mail_bytes.expect("Failed to parse mail")).unwrap();

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

    let parsed_date = DateTime::parse_from_rfc2822(&date)
        .unwrap()
        .with_timezone(&Local);

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
    pager
        .set_prompt("Reading email (Press 'q' to exit)")
        .expect("Failed to send data to the pager");
    pager
        .set_text(content)
        .expect("Failed to send data to the pager");

    minus::dynamic_paging(pager).expect("Failed to start pager");
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
        Err(_) => {
            eprintln!("Failed to select folder");
            process::exit(1);
        }
    }

    let mails = imap_session.fetch("1:*", "(FLAGS)").unwrap();

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

    let found_mail = imap_session
        .fetch(format!("{}", max_value - (id as usize)), "(BODY.PEEK[])")
        .unwrap();

    let mail_bytes = found_mail[0].body();

    let parsed = mailparse::parse_mail(mail_bytes.expect("Failed to parse mail")).unwrap();

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
                    file.write_all(&binary_data).expect("Failed to save file");
                }
            } else {
                let mut save_path = PathBuf::from(save_folder.clone());
                save_path.push(clean_filename);

                let binary_data = part.get_body_raw().unwrap();

                let mut file = File::create(&save_path).unwrap();
                file.write_all(&binary_data).expect("Failed to save file");
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
    let mut imap_session = auth(config);

    if folder != "ALL" {
        match imap_session.select(&folder) {
            Ok(_) => {}
            Err(_) => {
                eprintln!("Failed to select folder");
                process::exit(1);
            }
        }

        let fetch = imap_session.fetch("1:*", "(BODY.PEEK[])").unwrap();

        let mut current_id = 0;

        for mail in fetch.iter().rev() {
            let parsed = mailparse::parse_mail(mail.body().expect("Failed to parse mail")).unwrap();

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
                println!("[{:03}] | {} | {}", current_id, subject, from);
            }
            current_id += 1;
        }
    } else {
        let mailboxes = imap_session.list(None, Some("*")).unwrap();

        for mailbox in mailboxes.iter() {
            match imap_session.select(&mailbox.name()) {
                Ok(_) => {}
                Err(_) => {
                    eprintln!("Failed to select folder");
                    process::exit(1);
                }
            }

            let fetch = imap_session.fetch("1:*", "(BODY.PEEK[])").unwrap();

            let mut current_id = 0;

            for mail in fetch.iter().rev() {
                let parsed =
                    mailparse::parse_mail(mail.body().expect("Failed to parse email")).unwrap();

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
                    println!(
                        "[{:03}] | {} | {} | {}",
                        current_id,
                        mailbox.name(),
                        subject,
                        from
                    );
                }
                current_id += 1;
            }
        }
    }

    let _ = imap_session.logout();
}

// mv because move is reserved by rust
pub fn mv(config: Config, from: String, to: String, id: Vec<u32>) {
    let mut imap_session = auth(config);

    match imap_session.select(&from) {
        Ok(_) => {}
        Err(_) => {
            eprintln!("Failed to select folder");
        }
    }

    let mails = imap_session.fetch("1:*", "(FLAGS)").unwrap();

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
        let _ = imap_session.mv(&format!("{}", max_value - (*index as usize)), &to);
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

pub fn delete(config: Config, id: Option<Vec<u32>>, folder: String) {
    todo!("implement deleting");
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
