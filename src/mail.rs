use crate::{APP_NAME, Config, Account, account::auth};

use dialoguer::{theme::ColorfulTheme, Confirm, Select, Editor, Input};
use minus::Pager;
use keyring::Entry;
use chrono::{Local, DateTime};
use std::process;
use mailparse::{MailHeaderMap, ParsedMail, DispositionType};

pub fn send(config: Config, to: Option<Vec<String>>, subject: Option<String>, body: Option<String>, attachments: Option<Vec<std::path::PathBuf>>, yes: bool) {
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

    let _ = imap_session.select(folder);

    let mails = imap_session.fetch("1:*", "(BODY.PEEK[])").unwrap();

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

    let found_mail = imap_session.fetch(format!("{}", max_value - (id as usize)), "(BODY[])").unwrap();

    let mail_bytes = found_mail[0].body();

    let parsed = mailparse::parse_mail(mail_bytes.expect("Failed to parse mail")).unwrap();

    let subject = parsed.headers.get_first_value("Subject").unwrap_or_default();
    let from = parsed.headers.get_first_value("From").unwrap_or_default();
    let to = parsed.headers.get_first_value("To").unwrap_or_default();
    let date = parsed.headers.get_first_value("Date").unwrap_or_default();
    let cc = parsed.headers.get_first_value("Cc").unwrap_or_default();

    let html_body = find_html_text(&parsed);
    let plain_body = find_plain_text(&parsed);

    let body = match (html_body, plain_body) {
        (Some(html), _) => {
            let w = if let Some((w, _)) = term_size::dimensions() {
                w
            } else {
                80
            };
            august::convert(&html, w)
        },
        (None, Some(plain)) => {
            plain
        },
        (None, None) => " (No content) ".to_string(),
    };

    let parsed_date = DateTime::parse_from_rfc2822(&date).unwrap().with_timezone(&Local);

    let mut attachments: Vec<String> = Vec::new();

    for part in parsed.subparts {
        let disposition = part.get_content_disposition();

        if disposition.disposition == DispositionType::Attachment || disposition.params.contains_key("filename") {
            let name = disposition.params.get("filename")
                .cloned()
                .unwrap_or_else(|| "unnamed_file".to_string());

            attachments.push(name);
        }
    }

    let mut content: String = "".to_string();

    content.push_str(&format!("From:     {}\n", from));
    content.push_str(&format!("To:       {}\n", to));
    if !cc.is_empty() {
        content.push_str(&format!("Cc:    {}\n", cc));
    }
    content.push_str(&format!("Date:     {}\n", parsed_date.format("%Y-%m-%d %I:%M:%S %p")));
    content.push_str(&format!("Subject:  {}\n", subject));
    if !attachments.is_empty() {
        content.push_str(&format!("Attachments:  {}\n\n", attachments.join(", ")));
    }
    content.push_str(&format!("{}\n", body));

    let pager = Pager::new();
    pager.set_prompt("Reading email (Press 'q' to exit)").expect("Failed to send data to the pager");
    pager.set_text(content).expect("Failed to send data to the pager");

    minus::dynamic_paging(pager).expect("Failed to start pager");
}

pub fn search(config: Config, query: String, folder: String, since: Option<String>) {
    // validate date too
    todo!("Implement searching for {} in folder {}", query, folder);
}

// mv because move is reserved by rust
pub fn mv(config: Config, id: u32, from: String, to: String) {
    todo!("Implement moving mail with index {} from folder {} to folder {}", id, from, to);
}

pub fn draft(config: Config, to: Option<Vec<String>>, subject: Option<String>, body: Option<String>, attachments: Option<Vec<std::path::PathBuf>>) {
    todo!("Implement drafts");
}
