/*
    ion-mail  Mail CLI in Rust supporting all mail functions & OAuth2 login
    Copyright (C) 2026  Vektrace

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as
    published by the Free Software Foundation, either version 3 of the
    License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use crate::{APP_NAME, Account, Config, Endpoints, Imap, MailConfig, Smtp, account::auth};

use chrono::{DateTime, Local, Utc};
use dialoguer::{Confirm, Editor, Input, MultiSelect, Select, theme::ColorfulTheme};
use keyring_core::Entry;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{Message, SmtpTransport, Transport};
use mailparse::{DispositionType, MailHeaderMap, ParsedMail};
use minus::Pager;
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, ClientId, DeviceAuthorizationUrl, RefreshToken, Scope,
    StandardDeviceAuthorizationResponse, TokenResponse, TokenUrl,
};
use std::fs;
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
    let mut found = false;

    let smtp = Smtp {
        host: "".to_string(),
        port: 0,
        security: "".to_string(),
    };
    let imap = Imap {
        host: "".to_string(),
        port: 0,
        security: "".to_string(),
    };
    let mut use_account = Account {
        id: 0,
        email: "".to_string(),
        active: false,
        smtp,
        imap,
        oidc: None,
        scopes: None,
        token_expiration: None,
    };

    for account in &config.accounts {
        if account.active {
            found = true;
            use_account = account.clone();
        }
    }

    if !found {
        println!("No account is currently active");
        keyring_core::unset_default_store();
        process::exit(0);
    }

    let email_str = use_account.email.as_str().parse().unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    // also have to do like all checks and if only some are done still open interactively
    let mail_components = if let Some(ref to) = to
        && let Some(ref subject) = subject
        && let Some(ref body) = body
    {
        let mut paths = Vec::new();

        if let Some(attachments) = attachments {
            for attachment_path in attachments.iter() {
                let attachment = attachment_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap();
                if !attachment.is_empty() {
                    let expanded = shellexpand::tilde(&attachment).to_string();
                    if PathBuf::from(expanded.clone()).exists() {
                        if PathBuf::from(expanded.clone()).is_file() {
                            match PathBuf::from(expanded).canonicalize() {
                                Ok(p) => paths.push(p),
                                Err(e) => {
                                    eprintln!("Unexpected Error: {}", e);
                                    keyring_core::unset_default_store();
                                    process::exit(1);
                                }
                            }
                        } else {
                            eprintln!("Path is not a file");
                            keyring_core::unset_default_store();
                            process::exit(1);
                        }
                    } else {
                        eprintln!("Path does not exist");
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                }
            }
        }

        let final_paths = if paths.is_empty() { None } else { Some(paths) };

        (
            to.to_vec(),
            subject.to_string(),
            body.to_string(),
            final_paths,
        )
    } else {
        let to: Vec<String> = if let Some(ref to) = to {
            to.to_vec()
        } else {
            let to_joined: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter the recipients of the email (separated by spaces)")
                .validate_with(|input: &String| -> Result<(), &str> {
                    if input.is_empty() {
                        Err("Recipients cannot be empty")
                    } else {
                        Ok(())
                    }
                })
                .interact()
                .unwrap();

            to_joined
                .split(' ')
                .map(|n| n.to_string())
                .collect::<Vec<String>>()
        };

        let subject: String = if let Some(ref subject) = subject {
            subject.to_string()
        } else {
            Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter the subject of the email")
                .interact()
                .unwrap()
        };

        let body = if let Some(ref body) = body {
            body.to_string()
        } else {
            let edited = Editor::new().edit("Enter the body of the email").unwrap_or_else(|err| {
                    eprintln!(
                        "Failed to open Editor: ({}), Maybe the EDITOR environment variable is not set?",
                        err
                    );
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

            if let Some(body) = edited {
                body
            } else {
                eprintln!("Body cannot be empty");
                keyring_core::unset_default_store();
                process::exit(1);
            }
        };

        let mut paths = Vec::new();

        if let Some(attachments) = attachments {
            for attachment_path in attachments.iter() {
                let attachment = attachment_path
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap();
                if !attachment.is_empty() {
                    let expanded = shellexpand::tilde(&attachment).to_string();
                    if PathBuf::from(expanded.clone()).exists() {
                        if PathBuf::from(expanded.clone()).is_file() {
                            match PathBuf::from(expanded).canonicalize() {
                                Ok(p) => paths.push(p),
                                Err(e) => {
                                    eprintln!("Unexpected Error: {}", e);
                                    keyring_core::unset_default_store();
                                    process::exit(1);
                                }
                            }
                        } else {
                            eprintln!("Path is not a file");
                            keyring_core::unset_default_store();
                            process::exit(1);
                        }
                    } else {
                        eprintln!("Path does not exist");
                        keyring_core::unset_default_store();
                        process::exit(1);
                    }
                }
            }
        }

        loop {
            let input: String = Input::new()
                .with_prompt("Enter a path (or leave blank to finish)")
                .allow_empty(true)
                .validate_with(|input: &String| -> Result<(), &str> {
                    if !input.is_empty() {
                        let expanded = shellexpand::tilde(input).to_string();
                        if PathBuf::from(expanded.clone()).exists() {
                            if PathBuf::from(expanded.clone()).is_file() {
                                Ok(())
                            } else {
                                Err("Path is not a file")
                            }
                        } else {
                            Err("Path does not exist")
                        }
                    } else {
                        Ok(())
                    }
                })
                .interact_text()
                .unwrap();

            if input.is_empty() {
                break;
            }

            let expanded = shellexpand::tilde(&input).to_string();
            match PathBuf::from(expanded).canonicalize() {
                Ok(p) => paths.push(p),
                Err(e) => {
                    eprintln!("Unexpected Error: {}", e);
                    process::exit(1);
                }
            }
        }

        let final_paths = if paths.is_empty() { None } else { Some(paths) };

        (to, subject, body, final_paths)
    };

    let to = mail_components.0;
    let subject = mail_components.1;
    let body = mail_components.2;
    let attachments = mail_components.3;

    let mut message = Message::builder().from(Mailbox::new(None, email_str));

    for i in to {
        let current_to = i.as_str().parse().unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

        message = message.to(Mailbox::new(None, current_to));
    }

    message = message.subject(&subject);

    let alternative_part = MultiPart::alternative()
        .singlepart(SinglePart::plain(strip_markdown::strip_markdown(&body)))
        .singlepart(SinglePart::html(markdown::to_html(&body)));

    let mut multipart = MultiPart::mixed().multipart(alternative_part);

    let mut processed_files: Vec<(String, ContentType, Vec<u8>)> = Vec::new();

    if let Some(attachments) = attachments {
        for attachment in attachments {
            if attachment.as_path().is_file() {
                let name = attachment.as_path().file_name();
                let guess = mime_guess::from_path(attachment.as_path()).first_or_octet_stream();
                let content_type =
                    ContentType::parse(&format!("{}/{}", guess.type_(), guess.subtype())).unwrap();
                let filebody = fs::read(attachment.as_path()).unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                if let Some(name) = name {
                    let name_str = name.to_str();
                    if let Some(name_str) = name_str {
                        processed_files.push((name_str.to_string(), content_type, filebody));
                    } else {
                        eprintln!("Failed to get file name");
                    }
                } else {
                    eprintln!("Failed to get file name");
                }
            } else {
                eprintln!("Attachment {} is not a file", attachment.display());
            }
        }
    }

    for (name, mime, content) in processed_files {
        let attachment = Attachment::new(name).body(content, mime);

        multipart = multipart.singlepart(attachment);
    }

    let final_message = message.multipart(multipart).unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    let entry = Entry::new(APP_NAME, &use_account.id.to_string()).unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    let mut password = entry.get_password().unwrap_or_else(|err| {
        eprintln!("Failed to get password: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    password = if let Some(token_expiration) = use_account.token_expiration {
        let stored_date = DateTime::parse_from_rfc3339(&token_expiration)
            .unwrap_or_else(|err| {
                eprintln!("Unexpected Error: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            })
            .with_timezone(&Utc);

        let today = Utc::now();

        if today >= stored_date {
            // relogin
            let client_id_entry = Entry::new(APP_NAME, &format!("{}.id", &use_account.id))
                .unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

            let client_id = client_id_entry.get_password().unwrap_or_else(|err| {
                eprintln!("Failed to get Client Id: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

            let domain = use_account.email.split("@").collect::<Vec<&str>>()[1];
            let config_entry = reqwest::blocking::get(format!(
                "https://raw.githubusercontent.com/Paulprojects8711/ion-mail-config/refs/heads/main/providers/{}.toml",
                domain
            )).unwrap_or_else(|err| {
                    eprintln!("Failed to get provider list: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

            let mail_config: MailConfig = if config_entry.status().is_success() {
                let config_text = config_entry.text().unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                toml::from_str(&config_text).unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                })
            } else {
                eprintln!("Failed to get provider list");
                keyring_core::unset_default_store();
                process::exit(1);
            };
            let refresh_token_entry =
                Entry::new(APP_NAME, &format!("{}.refresh_token", use_account.id)).unwrap_or_else(
                    |err| {
                        eprintln!("Unexpected Error: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    },
                );

            let refresh_token: Option<String> = refresh_token_entry.get_password().ok();
            let response = reqwest::blocking::get(&mail_config.oidc).unwrap_or_else(|err| {
                eprintln!("Failed to get OIDC Discovery Endpoints: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

            let response_text = response.text().unwrap_or_else(|err| {
                eprintln!("Unexpected Error: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

            let endpoints: Endpoints = serde_json::from_str(&response_text).unwrap_or_else(|err| {
                eprintln!("Unexpected Error: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

            let client = BasicClient::new(ClientId::new(client_id))
                .set_auth_uri(AuthUrl::new(endpoints.authorization_endpoint).unwrap())
                .set_token_uri(TokenUrl::new(endpoints.token_endpoint).unwrap())
                .set_device_authorization_url(
                    DeviceAuthorizationUrl::new(endpoints.device_authorization_endpoint).unwrap(),
                );
            let http_client = oauth2::reqwest::blocking::ClientBuilder::new()
                .redirect(oauth2::reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            if let Some(refresh_token) = refresh_token {
                let final_refresh_token = RefreshToken::new(refresh_token);
                let mut details = client.exchange_refresh_token(&final_refresh_token);
                if let Some(ref scopes) = use_account.scopes {
                    for scope in scopes {
                        details = details.add_scope(Scope::new(scope.to_string()));
                    }
                }
                let token_result = details.request(&http_client).unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                if let Some(expires_in) = token_result.expires_in() {
                    let expire_date = (today + expires_in).to_rfc3339();
                    use_account.token_expiration = Some(expire_date);
                    let mut _config = Config {
                        accounts: Vec::new(),
                    };
                    for acc in &config.accounts {
                        if !acc.active {
                            _config.accounts.push(acc.clone());
                        } else {
                            _config.accounts.push(use_account.clone());
                        }
                    }
                    let toml_output = toml::to_string(&_config).unwrap_or_else(|err| {
                        eprintln!("Unexpected Error: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                    let toml_p = dirs::config_local_dir()
                        .unwrap_or_else(|| {
                            eprintln!("Could not find home directory");
                            process::exit(1);
                        })
                        .join("ion-mail")
                        .join("config.toml");

                    let toml_path: &str = toml_p.to_str().unwrap();
                    fs::write(toml_path, toml_output).unwrap_or_else(|err| {
                        eprintln!("Error when saving new config: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });
                }
                if let Some(refresh_token) = token_result.refresh_token() {
                    refresh_token_entry
                        .set_password(refresh_token.secret())
                        .unwrap_or_else(|err| {
                            eprintln!("Failed to set password: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });
                }
                // save password
                entry
                    .set_password(token_result.access_token().secret())
                    .unwrap_or_else(|err| {
                        eprintln!("Failed to set password: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                token_result.access_token().clone().into_secret()
            } else {
                let mut details = client.exchange_device_code();

                if let Some(ref scopes) = use_account.scopes {
                    for scope in scopes {
                        details = details.add_scope(Scope::new(scope.to_string()));
                    }
                }
                let final_details: StandardDeviceAuthorizationResponse =
                    details.request(&http_client).unwrap_or_else(|err| {
                        eprintln!("Unexpected Error: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                println!(
                    "To authenticate, open this URL in your browser:\n{}\nand enter the code: {}",
                    final_details.verification_uri(),
                    final_details.user_code().secret()
                );

                let token_result = client
                    .exchange_device_access_token(&final_details)
                    .request(&http_client, std::thread::sleep, None)
                    .unwrap_or_else(|err| {
                        eprintln!("Unexpected Error: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                if let Some(expires_in) = token_result.expires_in() {
                    let expire_date = (today + expires_in).to_rfc3339();
                    use_account.token_expiration = Some(expire_date);
                    let mut _config = Config {
                        accounts: Vec::new(),
                    };
                    for acc in &config.accounts {
                        if !acc.active {
                            _config.accounts.push(acc.clone());
                        } else {
                            _config.accounts.push(use_account.clone());
                        }
                    }
                    let toml_output = toml::to_string(&_config).unwrap_or_else(|err| {
                        eprintln!("Unexpected Error: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                    let toml_p = dirs::config_local_dir()
                        .unwrap_or_else(|| {
                            eprintln!("Could not find home directory");
                            process::exit(1);
                        })
                        .join("ion-mail")
                        .join("config.toml");

                    let toml_path: &str = toml_p.to_str().unwrap();
                    fs::write(toml_path, toml_output).unwrap_or_else(|err| {
                        eprintln!("Error when saving new config: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });
                }
                if let Some(refresh_token) = token_result.refresh_token() {
                    refresh_token_entry
                        .set_password(refresh_token.secret())
                        .unwrap_or_else(|err| {
                            eprintln!("Failed to set password: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });
                }
                // save password
                entry
                    .set_password(token_result.access_token().secret())
                    .unwrap_or_else(|err| {
                        eprintln!("Failed to set password: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                token_result.access_token().clone().into_secret()
            }
        } else {
            password
        }
    } else {
        password
    };

    let mut send = false;
    if Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Are you sure you want to send this Email?")
        .default(true)
        .show_default(true)
        .wait_for_newline(true)
        .interact()
        .unwrap()
    {
        send = true;
    }

    if yes || send {
        let creds = Credentials::new(use_account.email, password);

        let mailer = if use_account.smtp.security == "SSL" || use_account.smtp.security == "TLS" {
            let mailer =
                SmtpTransport::relay(&use_account.smtp.host.clone()).unwrap_or_else(|err| {
                    eprintln!("Failed to connect via SSL/TLS: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

            if use_account.oidc.is_some() {
                mailer
                    .credentials(creds)
                    .port(use_account.smtp.port)
                    .authentication(vec![Mechanism::Xoauth2])
                    .build()
            } else {
                mailer
                    .credentials(creds)
                    .port(use_account.smtp.port)
                    .build()
            }
        } else if use_account.smtp.security == "STARTTLS" {
            let mailer = SmtpTransport::starttls_relay(&use_account.smtp.host.clone())
                .unwrap_or_else(|err| {
                    eprintln!("Failed to connect via SSL/TLS: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

            if use_account.oidc.is_some() {
                mailer
                    .credentials(creds)
                    .port(use_account.smtp.port)
                    .authentication(vec![Mechanism::Xoauth2])
                    .build()
            } else {
                mailer
                    .credentials(creds)
                    .port(use_account.smtp.port)
                    .build()
            }
        } else {
            eprintln!("Security type not recognized");
            keyring_core::unset_default_store();
            process::exit(1);
        };

        match mailer.send(&final_message) {
            Ok(_) => println!("Email sent"),
            Err(e) => {
                eprintln!("Unexpected Error: {}", e);
                keyring_core::unset_default_store();
                process::exit(1);
            }
        }
    } else {
        println!("Email discarded");
    }
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

    imap_session.select(folder).unwrap_or_else(|err| {
        eprintln!("Failed to select folder: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    let mails = imap_session.fetch("1:*", "(FLAGS)").unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    let mut found = false;

    for (current_id, _mail) in mails.iter().rev().enumerate() {
        if current_id == id as usize {
            found = true;
        }
    }

    if !found {
        eprintln!("Mail with ID {} could not be found", id);
        process::exit(1);
    }

    let max_value = mails.len();

    let found_mail = imap_session
        .fetch(format!("{}", max_value - (id as usize)), "(BODY[])")
        .unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

    imap_session.logout().unwrap_or_else(|err| {
        eprintln!("Logout failed, ignoring... ({})", err);
    });

    let mail_bytes = found_mail[0].body();

    let parsed = mailparse::parse_mail(mail_bytes.unwrap()).unwrap_or_else(|err| {
        eprintln!("Failed to parse mail: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

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
        (Some(html), Some(plain)) => {
            let md = html2md::parse_html(&html);
            let skin = MadSkin::default();

            let original_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));

            let result = std::panic::catch_unwind(|| skin.term_text(&md).to_string());

            std::panic::set_hook(original_hook);

            match result {
                Ok(term_md) => term_md,
                Err(_) => plain,
            }
        }
        (Some(html), None) => {
            let md = html2md::parse_html(&html);
            let skin = MadSkin::default();

            let original_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));

            let result = std::panic::catch_unwind(|| skin.term_text(&md).to_string());

            std::panic::set_hook(original_hook);

            match result {
                Ok(term_md) => term_md,
                Err(_) => md,
            }
        }
        (None, Some(plain)) => plain,
        (None, None) => " (No content) ".to_string(),
    };

    let parsed_date = DateTime::parse_from_rfc2822(&date)
        .unwrap_or_else(|err| {
            eprintln!("Failed to parse date: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        })
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
        .unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

    pager.set_text(content).unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    minus::dynamic_paging(pager).unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });
}

pub fn download(
    config: Config,
    folder: String,
    id: u32,
    attachment_id: Option<Vec<u32>>,
    save_folder: String,
) {
    let mut imap_session = auth(config);

    imap_session.select(folder).unwrap_or_else(|err| {
        eprintln!("Failed to select folder: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    let mails = imap_session.fetch("1:*", "(FLAGS)").unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    let mut found = false;

    for (current_id, _mail) in mails.iter().rev().enumerate() {
        if current_id == id as usize {
            found = true;
        }
    }

    if !found {
        eprintln!("Mail with ID {} could not be found", id);
        process::exit(1);
    }

    let max_value = mails.len();

    let found_mail = imap_session
        .fetch(format!("{}", max_value - (id as usize)), "(BODY.PEEK[])")
        .unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

    let mail_bytes = found_mail[0].body();

    let parsed = mailparse::parse_mail(mail_bytes.unwrap()).unwrap_or_else(|err| {
        eprintln!("Failed to parse mail: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

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
                    file.write_all(&binary_data).unwrap_or_else(|err| {
                        eprintln!("Failed to save file: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });
                }
            } else {
                let mut save_path = PathBuf::from(save_folder.clone());
                save_path.push(clean_filename);

                let binary_data = part.get_body_raw().unwrap();

                let mut file = File::create(&save_path).unwrap();
                file.write_all(&binary_data).unwrap_or_else(|err| {
                    eprintln!("Failed to save file: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });
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
        imap_session.select(&folder).unwrap_or_else(|err| {
            eprintln!("Failed to select folder: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

        let fetch = imap_session
            .fetch("1:*", "(BODY.PEEK[])")
            .unwrap_or_else(|err| {
                eprintln!("Unexpected Error: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

        for (current_id, mail) in fetch.iter().rev().enumerate() {
            let parsed = mailparse::parse_mail(mail.body().unwrap()).unwrap_or_else(|err| {
                eprintln!("Failed to parse mail: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

            let subject = parsed
                .headers
                .get_first_value("Subject")
                .unwrap_or_default();
            let from = parsed.headers.get_first_value("From").unwrap_or_default();
            let body = find_plain_text(&parsed).unwrap_or_default();

            if subject.contains(&query) || from.contains(&query) || body.contains(&query) {
                results.push((
                    format!("[{:03}] | {} | {}", current_id, subject, from),
                    folder.clone(),
                ));
            }
        }
    } else {
        let mailboxes = imap_session.list(None, Some("*")).unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

        for mailbox in mailboxes.iter() {
            imap_session.select(mailbox.name()).unwrap_or_else(|err| {
                eprintln!("Failed to select folder: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

            let fetch = imap_session
                .fetch("1:*", "(BODY.PEEK[])")
                .unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

            for (current_id, mail) in fetch.iter().rev().enumerate() {
                let parsed = mailparse::parse_mail(mail.body().unwrap()).unwrap_or_else(|err| {
                    eprintln!("Failed to parse mail: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                let subject = parsed
                    .headers
                    .get_first_value("Subject")
                    .unwrap_or_default();
                let from = parsed.headers.get_first_value("From").unwrap_or_default();
                let body = find_plain_text(&parsed).unwrap_or_default();

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
            }
        }
    }

    if results.is_empty() {
        println!("No results found");
        process::exit(0);
    }

    let selection = Select::with_theme(&ColorfulTheme::default())
        .default(0)
        .items(results.iter().map(|n| n.0.clone()).collect::<Vec<String>>())
        .clear(false)
        .max_length(20)
        .interact_opt()
        .unwrap();

    if let Some(selection) = selection {
        let s = selection.try_into().unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });
        read(config, results[s as usize].1.clone(), s);
    }

    imap_session.logout().unwrap_or_else(|err| {
        eprintln!("Logout failed: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });
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
        imap_session.select(&from).unwrap_or_else(|err| {
            eprintln!("Failed to select folder: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

        let mails = imap_session.fetch("1:*", "(FLAGS)").unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

        let mut found_ids = Vec::new();

        for (current_id, _mail) in mails.iter().rev().enumerate() {
            if id.contains(&current_id.try_into().unwrap()) {
                found_ids.push(current_id);
            }
        }

        if found_ids.len() != id.len() {
            eprintln!(
                "Mail with ID {} could not be found",
                id.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );
            keyring_core::unset_default_store();
            process::exit(1);
        }

        let max_value = mails.len();

        for index in &id {
            imap_session
                .mv(format!("{}", max_value - (*index as usize)), &to)
                .unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                });
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

    imap_session.select(&folder).unwrap_or_else(|err| {
        eprintln!("Failed to select folder: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    if let Some(id) = id {
        let mails = imap_session.fetch("1:*", "(UID)").unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

        let mut found_ids = Vec::new();

        for (current_id, mail) in mails.iter().rev().enumerate() {
            if id.contains(&current_id.try_into().unwrap())
                && let Some(uid) = mail.uid
            {
                found_ids.push(uid);
            }
        }

        if found_ids.len() != id.len() {
            eprintln!(
                "Mail with ID {} could not be found",
                id.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );
            keyring_core::unset_default_store();
            process::exit(1);
        }

        if Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Are you sure you want to delete mail?")
            .default(true)
            .show_default(true)
            .wait_for_newline(true)
            .interact()
            .unwrap()
        {
            let uid_set = found_ids
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<String>>()
                .join(",");

            imap_session
                .uid_store(&uid_set, "FLAGS (\\Deleted)")
                .unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

            imap_session.uid_expunge(&uid_set).unwrap_or_else(|err| {
                eprintln!("Unexpected Error: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });
        }
    } else {
        let mails = imap_session
            .fetch("1:*", "(UID BODY.PEEK[HEADER])")
            .unwrap_or_else(|err| {
                eprintln!("Unexpected Error: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

        let mut current_id = 0;
        let mut messages: Vec<String> = Vec::new();

        for mail in mails.iter().rev() {
            if let Some(header_bytes) = mail.header() {
                let (parsed_headers, _) =
                    mailparse::parse_headers(header_bytes).unwrap_or_else(|err| {
                        eprintln!("Failed to parse headers: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

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
                    .unwrap_or_else(|err| {
                        eprintln!("Failed to parse date: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    })
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

        if !messages.is_empty() {
            let selection = MultiSelect::with_theme(&ColorfulTheme::default())
                .items(&messages)
                .max_length(20)
                .interact_opt()
                .unwrap();

            if let Some(selection) = selection {
                let mut found_ids = Vec::new();

                for (current_id, mail) in mails.iter().rev().enumerate() {
                    if selection.contains(&current_id)
                        && let Some(uid) = mail.uid
                    {
                        found_ids.push(uid);
                    }
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
                    keyring_core::unset_default_store();
                    process::exit(1);
                }

                if Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("Are you sure you want to delete mail?")
                    .default(true)
                    .show_default(true)
                    .wait_for_newline(true)
                    .interact()
                    .unwrap()
                {
                    let uid_set = found_ids
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<String>>()
                        .join(",");

                    imap_session
                        .uid_store(&uid_set, "FLAGS (\\Deleted)")
                        .unwrap_or_else(|err| {
                            eprintln!("Unexpected Error: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });

                    imap_session.uid_expunge(&uid_set).unwrap_or_else(|err| {
                        eprintln!("Unexpected Error: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });
                }
            }
            println!("Mail deleted successfully");
        } else {
            println!("Folder {} is empty", folder);
        }
    }

    imap_session.logout().unwrap_or_else(|err| {
        eprintln!("Logout failed, ignoring... ({})", err);
    });
}
