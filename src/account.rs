/*
    ion-mail  Mail CLI in Rust supporting all mail functions & OAuth2 login
    Copyright (C) 2026  Paul8711

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

use crate::{APP_NAME, Account, Config, Endpoints, Imap, MailConfig, Smtp};

use chrono::{DateTime, Utc};
use dialoguer::{Confirm, Input, Password, Select, theme::ColorfulTheme};
use email_address::EmailAddress;
use keyring_core::Entry;
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, ClientId, DeviceAuthorizationUrl, RefreshToken, Scope,
    StandardDeviceAuthorizationResponse, TokenResponse, TokenUrl,
};
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::process;
use std::time::Duration;

struct OAuth2 {
    user: String,
    access_token: String,
}

impl imap::Authenticator for OAuth2 {
    type Response = String;
    fn process(&self, _: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.user, self.access_token
        )
    }
}

pub fn auth(config: Config) -> imap::Session<native_tls::TlsStream<std::net::TcpStream>> {
    let mut found = false;

    let imap = Imap {
        host: "".to_string(),
        port: 0,
        security: "".to_string(),
    };

    let smtp = Smtp {
        host: "".to_string(),
        port: 0,
        security: "".to_string(),
    };

    let mut use_account = Account {
        id: 0,
        active: false,
        email: "".to_string(),
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
                        _config.accounts.push(acc.clone());
                    }
                    let toml_output = toml::to_string(&_config).unwrap_or_else(|err| {
                        eprintln!("Unexpected Error: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                    let toml_p = dirs::config_local_dir()
                        .unwrap_or_else(|| {
                            eprintln!("Could not find config directory");
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
                            eprintln!("Could not find config directory");
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

    let tls = native_tls::TlsConnector::builder().build().unwrap();

    let client = if use_account.imap.security == "SSL" || use_account.imap.security == "TLS" {
        imap::connect(
            (use_account.imap.host.clone(), use_account.imap.port),
            &use_account.imap.host,
            &tls,
        )
        .unwrap_or_else(|err| {
            eprintln!("Failed to connect via SSL/TLS: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        })
    } else if use_account.imap.security == "STARTTLS" {
        imap::connect_starttls(
            (use_account.imap.host.clone(), use_account.imap.port),
            &use_account.imap.host,
            &tls,
        )
        .unwrap_or_else(|err| {
            eprintln!("Failed to connect via STARTTLS: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        })
    } else {
        eprintln!("Security type not recognized");
        keyring_core::unset_default_store();
        process::exit(1);
    };

    if use_account.oidc.is_some() {
        let auth = OAuth2 {
            user: use_account.email,
            access_token: password,
        };

        client
            .authenticate("XOAUTH2", &auth)
            .unwrap_or_else(|(err, _orig_client)| {
                eprintln!("Login failed: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            })
    } else {
        client
            .login(use_account.email, password)
            .unwrap_or_else(|(err, _orig_client)| {
                eprintln!("Login failed: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            })
    }
}

pub fn add(toml_path: &str, old_config: Config) {
    if old_config.accounts.len() >= (u32::MAX - 10).try_into().unwrap() {
        eprintln!("Too many accounts registered, remove some then try again");
        keyring_core::unset_default_store();
        process::exit(1);
    }
    let security = vec!["SSL".to_string(), "STARTTLS".to_string()];
    let mail: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Email")
        .validate_with({
            move |input: &String| -> Result<(), &str> {
                if EmailAddress::is_valid(input) {
                    Ok(())
                } else {
                    Err("Invalid email address")
                }
            }
        })
        .interact_text()
        .unwrap();

    let domain = mail.split("@").collect::<Vec<&str>>()[1];

    let config_entry = reqwest::blocking::get(format!(
        "https://raw.githubusercontent.com/Paulprojects8711/ion-mail-config/refs/heads/main/providers/{}.toml",
        domain
    )).unwrap_or_else(|err| {
            eprintln!("Failed to get provider list: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

    let mut refresh_token = None;
    let mut token_expiration: Option<String> = None;
    let (mut new_account, client_id, password) = if config_entry.status().is_success() {
        let config_text = config_entry.text().unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

        let mail_config: MailConfig = toml::from_str(&config_text).unwrap_or_else(|err| {
            eprintln!("Unexpected Error: {}", err);
            keyring_core::unset_default_store();
            process::exit(1);
        });

        let smtp_host = mail_config.smtp.host;
        let smtp_port = mail_config.smtp.port;
        let smtp_security = mail_config.smtp.security;
        let imap_host = mail_config.imap.host;
        let imap_port = mail_config.imap.port;
        let imap_security = mail_config.imap.security;
        let oidc = mail_config.oidc.clone();
        let scopes = mail_config.scopes;
        let client_id: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Client ID")
            .interact_text()
            .unwrap();
        let final_client_id = Some(client_id.clone());

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

        let mut details = client.exchange_device_code();
        for scope in &scopes {
            details = details.add_scope(Scope::new(scope.to_string()));
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
            let expire_date = (Utc::now() + expires_in).to_rfc3339();
            token_expiration = Some(expire_date);
        }
        if let Some(token) = token_result.refresh_token() {
            refresh_token = Some(token.clone().into_secret());
        }
        let smtp = Smtp {
            host: smtp_host,
            port: smtp_port,
            security: smtp_security,
        };
        let imap = Imap {
            host: imap_host,
            port: imap_port,
            security: imap_security,
        };
        (
            Account {
                id: 0,
                email: mail.clone(),
                active: false,
                smtp,
                imap,
                oidc: Some(oidc),
                scopes: Some(scopes),
                token_expiration,
            },
            final_client_id,
            token_result.access_token().clone().into_secret(),
        )
    } else {
        let smtp_host: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("SMTP Server")
            .validate_with({
                move |input: &String| -> Result<(), &str> {
                    let target = format!("{}:0", input);

                    match target.to_socket_addrs() {
                        Ok(_) => Ok(()),
                        Err(_) => Err("Hostname could not be resolved"),
                    }
                }
            })
            .interact_text()
            .unwrap();

        let smtp_value = smtp_host.clone();
        let smtp_port: u16 = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("SMTP Port")
            .with_initial_text("587")
            .validate_with({
                move |input: &u16| -> Result<(), String> {
                    let target = format!("{}:{}", smtp_value, input);

                    if let Ok(mut addrs) = target.to_socket_addrs()
                        && let Some(address) = addrs.next()
                        && TcpStream::connect_timeout(&address, Duration::from_secs(3)).is_ok()
                    {
                        return Ok(());
                    }
                    Err(format!("Could not connect to port {}", input))
                }
            })
            .interact_text()
            .unwrap();

        let smtp_security_idx: usize = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("SMTP Security")
            .default(0)
            .items(&security)
            .interact()
            .unwrap();

        let smtp_security = security[smtp_security_idx].clone();

        let imap_host: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("IMAP Server")
            .validate_with({
                move |input: &String| -> Result<(), &str> {
                    let target = format!("{}:0", input);

                    match target.to_socket_addrs() {
                        Ok(_) => Ok(()),
                        Err(_) => Err("Hostname could not be resolved"),
                    }
                }
            })
            .interact_text()
            .unwrap();

        let imap_value = imap_host.clone();
        let imap_port: u16 = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("IMAP Port")
            .with_initial_text("993")
            .validate_with({
                move |input: &u16| -> Result<(), String> {
                    let target = format!("{}:{}", imap_value, input);

                    if let Ok(mut addrs) = target.to_socket_addrs()
                        && let Some(address) = addrs.next()
                        && TcpStream::connect_timeout(&address, Duration::from_secs(3)).is_ok()
                    {
                        return Ok(());
                    }
                    Err(format!("Could not connect to port {}", input))
                }
            })
            .interact_text()
            .unwrap();

        let imap_security_idx: usize = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("IMAP Security")
            .default(0)
            .items(&security)
            .interact()
            .unwrap();

        let imap_security = security[imap_security_idx].clone();

        let oauth2 = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Has OAuth2?")
            .default(true)
            .show_default(true)
            .wait_for_newline(true)
            .interact()
            .unwrap();

        let oidc = if oauth2 {
            Some(
                Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("OIDC Discovery Endpoint")
                    .interact()
                    .unwrap(),
            )
        } else {
            None
        };

        let scopes = if oidc.is_some() {
            let mut _scopes = Vec::new();
            loop {
                let scope: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Scope ('q' to exit)")
                    .interact()
                    .unwrap();
                if scope != "q" {
                    _scopes.push(scope);
                } else {
                    break;
                }
            }
            Some(_scopes)
        } else {
            None
        };

        let client_id = if oauth2 {
            Some(
                Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Client ID")
                    .interact()
                    .unwrap(),
            )
        } else {
            None
        };
        let client_id = client_id.clone();

        let password = if !oauth2 {
            Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Password")
                .validate_with(|input: &String| -> Result<(), String> {
                    let tls = native_tls::TlsConnector::builder().build().unwrap();

                    let client = if imap_security == "SSL" || imap_security == "TLS" {
                        imap::connect((imap_host.clone(), imap_port), &imap_host, &tls)
                            .unwrap_or_else(|err| {
                                eprintln!("Failed to connect via SSL/TLS: {}", err);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            })
                    } else if imap_security == "STARTTLS" {
                        imap::connect_starttls((imap_host.clone(), imap_port), &imap_host, &tls)
                            .unwrap_or_else(|err| {
                                eprintln!("Failed to connect via STARTTLS: {}", err);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            })
                    } else {
                        eprintln!("Security type not recognized");
                        keyring_core::unset_default_store();
                        process::exit(1);
                    };

                    match client.login(mail.clone(), input) {
                        Ok(_session) => Ok(()),
                        Err((e, _unauth_client)) => Err(format!("Error: {}", e)),
                    }
                })
                .interact()
                .unwrap()
        } else {
            let client_id = client_id.clone().expect("Client ID should exist");
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

            let mut details = client.exchange_device_code();

            if let Some(ref scopes) = scopes {
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

            if Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("Entry does not yet exist in database. Do you want to add it?")
                .default(true)
                .show_default(true)
                .wait_for_newline(true)
                .interact()
                .unwrap()
            {
                let formatted_smtp_security = if smtp_security == "SSL" {
                    "SSL%2FTLS"
                } else {
                    "STARTTLS"
                };
                let formatted_imap_security = if imap_security == "SSL" {
                    "SSL%2FTLS"
                } else {
                    "STARTTLS"
                };
                let link = format!(
                    "https://github.com/Paulprojects8711/ion-mail-config/issues/new?template=provider.yml&title=Add+provider:+{}&provider={}&smtp-host={}&smtp-port={}&smtp-security={}&imap-host={}&imap-port={}&imap-security={}&oidc={}&scopes={}",
                    domain,
                    domain,
                    smtp_host,
                    smtp_port,
                    formatted_smtp_security,
                    imap_host,
                    imap_port,
                    formatted_imap_security,
                    oidc.clone().expect("OIDC should exist"),
                    scopes.clone().expect("Scopes should exist").join("%0A"),
                );
                println!(
                    "To add the entry, open this link in a browser of choice: {}",
                    link
                );
            }
            if let Some(expires_in) = token_result.expires_in() {
                let expire_date = (Utc::now() + expires_in).to_rfc3339();
                token_expiration = Some(expire_date);
            }
            if let Some(token) = token_result.refresh_token() {
                refresh_token = Some(token.clone().into_secret());
            }
            token_result.access_token().clone().into_secret()
        };
        let smtp = Smtp {
            host: smtp_host,
            port: smtp_port,
            security: smtp_security,
        };
        let imap = Imap {
            host: imap_host,
            port: imap_port,
            security: imap_security,
        };
        (
            Account {
                id: 0,
                email: mail.clone(),
                active: false,
                smtp,
                imap,
                oidc,
                scopes,
                token_expiration,
            },
            client_id,
            password,
        )
    };

    // the plan here: save password as password to keyring and email and other config to a file
    // somewhere else
    // so keyring would look like: <id here>@ion-mail (i think) and password ****

    let mut new_id: u32 = 0;

    let mut current_active = true;

    for item in &old_config.accounts {
        if item.email == mail
            && item.smtp.host == new_account.smtp.host
            && item.smtp.port == new_account.smtp.port
            && item.smtp.security == new_account.smtp.security
            && item.imap.host == new_account.imap.host
            && item.imap.port == new_account.imap.port
            && item.imap.security == new_account.imap.security
            && item.oidc == new_account.oidc
            && item.scopes == new_account.scopes
        {
            eprintln!("Account already exists");
            keyring_core::unset_default_store();
            process::exit(1);
        }
        if item.active {
            current_active = false;
        }
        if item.id >= new_id {
            new_id = item.id + 1;
            if new_id >= (u32::MAX - 10) {
                eprintln!("ID too large");
                keyring_core::unset_default_store();
                process::exit(1);
            }
        }
    }

    let mut _config = old_config;

    new_account.id = new_id;
    new_account.active = current_active;

    _config.accounts.push(new_account);

    let toml_output = toml::to_string(&_config).unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    fs::write(toml_path, toml_output).unwrap_or_else(|err| {
        eprintln!("Error when saving new config: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    let entry = Entry::new(APP_NAME, &new_id.to_string()).unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    entry.set_password(&password).unwrap_or_else(|err| {
        eprintln!("Failed to set password: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    if let Some(refresh_token) = refresh_token {
        let refresh_token_entry = Entry::new(APP_NAME, &format!("{}.refresh_token", &new_id))
            .unwrap_or_else(|err| {
                eprintln!("Unexpected Error: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

        refresh_token_entry
            .set_password(&refresh_token)
            .unwrap_or_else(|err| {
                eprintln!("Failed to set password: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });
    }
    if let Some(client_id) = client_id {
        let client_id_entry =
            Entry::new(APP_NAME, &format!("{}.id", &new_id)).unwrap_or_else(|err| {
                eprintln!("Unexpected Error: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

        client_id_entry
            .set_password(&client_id)
            .unwrap_or_else(|err| {
                eprintln!("Failed to get Client Id: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });
    }
}

pub fn list(config: Config) {
    if !config.accounts.is_empty() {
        for account in config.accounts {
            println!(
                "[{id:03}] [{status}] {email}",
                id = account.id,
                status = if account.active { "+" } else { "-" },
                email = account.email
            );
        }
    } else {
        println!("No accounts found");
    }
}

// not use because rust complained
pub fn switch(toml_path: &str, config: Config, account: String) {
    let mut _config = Config {
        accounts: Vec::new(),
    };

    if let Ok(id) = account.parse::<u32>() {
        // loop through all registered, if the current active is found its active flag will be set
        // to false, if the entered id is found its active flag will be set to true
        let mut found = false;
        for mut acc in config.accounts {
            if acc.active {
                acc.active = false;
            }
            if acc.id == id {
                acc.active = true;
                found = true;
            }
            _config.accounts.push(acc);
        }
        if !found {
            eprintln!("Account with ID {} could not be found", id);
            keyring_core::unset_default_store();
            process::exit(1);
        }
    } else {
        // in this case just continue with string
        let mut found = false;
        for mut acc in config.accounts {
            if acc.active {
                acc.active = false;
            }
            if acc.email == account {
                acc.active = true;
                found = true;
            }
            _config.accounts.push(acc);
        }
        if !found {
            eprintln!("Account with email {} could not be found", account);
            process::exit(1);
        }
    }
    let toml_output = toml::to_string(&_config).unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    fs::write(toml_path, toml_output).unwrap_or_else(|err| {
        eprintln!("Error when saving new config: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    println!("Successfully switched to account {}", account);
}

pub fn whoami(config: Config) {
    if !config.accounts.is_empty() {
        for account in config.accounts {
            if account.active {
                println!(
                    "{id:03} | {email} | {smtp}:{smtp_port} ({smtp_security}) | {imap}:{imap_port} ({imap_security})",
                    id = account.id,
                    email = account.email,
                    smtp = account.smtp.host,
                    smtp_port = account.smtp.port,
                    smtp_security = account.smtp.security,
                    imap = account.imap.host,
                    imap_port = account.imap.port,
                    imap_security = account.imap.security
                );
                return;
            }
        }
        println!("No account is currently active");
    } else {
        println!("No accounts found");
    }
}

pub fn edit(toml_path: &str, old_config: Config, account: Option<String>) {
    let items = vec![
        "Email",
        "Password",
        "SMTP",
        "IMAP",
        "Save & Exit",
        "Save",
        "Exit",
    ];

    let mut config = Config {
        accounts: Vec::new(),
    };

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

    let mut account_edit = Account {
        id: 0,
        email: "".to_string(),
        active: false,
        smtp,
        imap,
        oidc: None,
        scopes: None,
        token_expiration: None,
    };

    if let Some(ref account) = account {
        if let Ok(id) = account.parse::<u32>() {
            for acc in old_config.accounts {
                if acc.id != id {
                    config.accounts.push(acc);
                } else {
                    account_edit = acc;
                    found = true;
                }
            }

            if !found {
                println!("Account with ID {} could not be found", id);
                keyring_core::unset_default_store();
                process::exit(1);
            }
        } else {
            for acc in old_config.accounts {
                if acc.email != *account {
                    config.accounts.push(acc);
                } else {
                    account_edit = acc;
                    found = true;
                }
            }

            if !found {
                println!("Account with email {} could not be found", account);
                keyring_core::unset_default_store();
                process::exit(1);
            }
        }
    } else {
        for acc in old_config.accounts {
            if !acc.active {
                config.accounts.push(acc);
            } else {
                account_edit = acc;
                found = true;
            }
        }

        if !found {
            println!("No account is currently active");
            keyring_core::unset_default_store();
            process::exit(1);
        }
    }

    let entry = Entry::new(APP_NAME, &account_edit.id.to_string()).unwrap_or_else(|err| {
        eprintln!("Unexpected Error: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    let mut password = entry.get_password().unwrap_or_else(|err| {
        eprintln!("Failed to get password: {}", err);
        keyring_core::unset_default_store();
        process::exit(1);
    });

    let security = vec!["SSL".to_string(), "STARTTLS".to_string()];
    let mut refresh_token: Option<String> = None;

    loop {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select element to edit")
            .default(0)
            .items(&items)
            .interact()
            .unwrap();

        match selection {
            0 => {
                account_edit.email = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Email")
                    .validate_with({
                        move |input: &String| -> Result<(), &str> {
                            if EmailAddress::is_valid(input) {
                                Ok(())
                            } else {
                                Err("Invalid email address")
                            }
                        }
                    })
                    .interact_text()
                    .unwrap();
            }
            1 => {
                password = if account_edit.oidc.is_none() {
                    Password::with_theme(&ColorfulTheme::default())
                        .with_prompt("Password")
                        .validate_with(|input: &String| -> Result<(), String> {
                            let tls = native_tls::TlsConnector::builder().build().unwrap();

                            let client = if account_edit.imap.security == "SSL"
                                || account_edit.imap.security == "TLS"
                            {
                                imap::connect(
                                    (account_edit.imap.host.clone(), account_edit.imap.port),
                                    &account_edit.imap.host,
                                    &tls,
                                )
                                .unwrap_or_else(|err| {
                                    eprintln!("Failed to connect via SSL/TLS: {}", err);
                                    keyring_core::unset_default_store();
                                    process::exit(1);
                                })
                            } else if account_edit.imap.security == "STARTTLS" {
                                imap::connect_starttls(
                                    (account_edit.imap.host.clone(), account_edit.imap.port),
                                    &account_edit.imap.host,
                                    &tls,
                                )
                                .unwrap_or_else(|err| {
                                    eprintln!("Failed to connect via STARTTLS: {}", err);
                                    keyring_core::unset_default_store();
                                    process::exit(1);
                                })
                            } else {
                                eprintln!("Security type not recognized");
                                keyring_core::unset_default_store();
                                process::exit(1);
                            };

                            match client.login(account_edit.email.clone(), input) {
                                Ok(_session) => Ok(()),
                                Err((e, _unauth_client)) => Err(format!("Error: {}", e)),
                            }
                        })
                        .interact()
                        .unwrap()
                } else {
                    let domain = account_edit.email.split("@").collect::<Vec<&str>>()[1];
                    let client_id_entry = Entry::new(APP_NAME, &format!("{}.id", &account_edit.id))
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
                    let response =
                        reqwest::blocking::get(&mail_config.oidc).unwrap_or_else(|err| {
                            eprintln!("Failed to get OIDC Discovery Endpoints: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });

                    let response_text = response.text().unwrap_or_else(|err| {
                        eprintln!("Unexpected Error: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                    let endpoints: Endpoints =
                        serde_json::from_str(&response_text).unwrap_or_else(|err| {
                            eprintln!("Unexpected Error: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });

                    let client = BasicClient::new(ClientId::new(client_id))
                        .set_auth_uri(AuthUrl::new(endpoints.authorization_endpoint).unwrap())
                        .set_token_uri(TokenUrl::new(endpoints.token_endpoint).unwrap())
                        .set_device_authorization_url(
                            DeviceAuthorizationUrl::new(endpoints.device_authorization_endpoint)
                                .unwrap(),
                        );
                    let http_client = oauth2::reqwest::blocking::ClientBuilder::new()
                        .redirect(oauth2::reqwest::redirect::Policy::none())
                        .build()
                        .unwrap();

                    let mut details = client.exchange_device_code();

                    if let Some(ref scopes) = account_edit.scopes {
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
                        let expire_date = (Utc::now() + expires_in).to_rfc3339();
                        account_edit.token_expiration = Some(expire_date);
                    }
                    if let Some(token) = token_result.refresh_token() {
                        refresh_token = Some(token.clone().into_secret());
                    }
                    token_result.access_token().clone().into_secret()
                };
            }
            2 => {
                account_edit.smtp.host = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("SMTP Server")
                    .validate_with({
                        move |input: &String| -> Result<(), &str> {
                            let target = format!("{}:0", input);

                            match target.to_socket_addrs() {
                                Ok(_) => Ok(()),
                                Err(_) => Err("Hostname could not be resolved"),
                            }
                        }
                    })
                    .interact_text()
                    .unwrap();

                let smtp_value = account_edit.smtp.host.clone();
                account_edit.smtp.port = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("SMTP Port")
                    .with_initial_text("587")
                    .validate_with({
                        move |input: &u16| -> Result<(), String> {
                            let target = format!("{}:{}", smtp_value, input);

                            if let Ok(mut addrs) = target.to_socket_addrs()
                                && let Some(address) = addrs.next()
                                && TcpStream::connect_timeout(&address, Duration::from_secs(3))
                                    .is_ok()
                            {
                                return Ok(());
                            }
                            Err(format!("Could not connect to port {}", input))
                        }
                    })
                    .interact_text()
                    .unwrap();

                let smtp_security_idx: usize = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("SMTP Security")
                    .default(0)
                    .items(&security)
                    .interact()
                    .unwrap();

                account_edit.smtp.security = security[smtp_security_idx].clone();
            }
            3 => {
                account_edit.imap.host = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("IMAP Server")
                    .validate_with({
                        move |input: &String| -> Result<(), &str> {
                            let target = format!("{}:0", input);

                            match target.to_socket_addrs() {
                                Ok(_) => Ok(()),
                                Err(_) => Err("Hostname could not be resolved"),
                            }
                        }
                    })
                    .interact_text()
                    .unwrap();

                let imap_value = account_edit.imap.host.clone();
                account_edit.imap.port = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("IMAP Port")
                    .with_initial_text("993")
                    .validate_with({
                        move |input: &u16| -> Result<(), String> {
                            let target = format!("{}:{}", imap_value, input);

                            if let Ok(mut addrs) = target.to_socket_addrs()
                                && let Some(address) = addrs.next()
                                && TcpStream::connect_timeout(&address, Duration::from_secs(3))
                                    .is_ok()
                            {
                                return Ok(());
                            }
                            Err(format!("Could not connect to port {}", input))
                        }
                    })
                    .interact_text()
                    .unwrap();

                let imap_security_idx: usize = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("IMAP Security")
                    .default(0)
                    .items(&security)
                    .interact()
                    .unwrap();

                account_edit.imap.security = security[imap_security_idx].clone();
            }
            4 => {
                config.accounts.push(account_edit.clone());
                let toml_output = toml::to_string(&config).unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                fs::write(toml_path, toml_output).unwrap_or_else(|err| {
                    eprintln!("Error when saving new config: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                entry.set_password(password.as_str()).unwrap_or_else(|err| {
                    eprintln!("Failed to set password: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                if let Some(refresh_token) = refresh_token {
                    let refresh_token_entry =
                        Entry::new(APP_NAME, &format!("{}.refresh_token", &account_edit.id))
                            .unwrap_or_else(|err| {
                                eprintln!("Unexpected Error: {}", err);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            });

                    refresh_token_entry
                        .set_password(&refresh_token)
                        .unwrap_or_else(|err| {
                            eprintln!("Failed to set password: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });
                }
                break;
            }
            5 => {
                config.accounts.push(account_edit.clone());
                let toml_output = toml::to_string(&config).unwrap_or_else(|err| {
                    eprintln!("Unexpected Error: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                fs::write(toml_path, toml_output).unwrap_or_else(|err| {
                    eprintln!("Error when saving new config: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                entry.set_password(password.as_str()).unwrap_or_else(|err| {
                    eprintln!("Failed to set password: {}", err);
                    keyring_core::unset_default_store();
                    process::exit(1);
                });

                if let Some(ref refresh_token) = refresh_token {
                    let refresh_token_entry =
                        Entry::new(APP_NAME, &format!("{}.refresh_token", &account_edit.id))
                            .unwrap_or_else(|err| {
                                eprintln!("Unexpected Error: {}", err);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            });

                    refresh_token_entry
                        .set_password(refresh_token)
                        .unwrap_or_else(|err| {
                            eprintln!("Failed to set password: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });
                }
            }
            _ => {
                if Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("Are you sure you want to exit?")
                    .default(true)
                    .show_default(true)
                    .wait_for_newline(true)
                    .interact()
                    .unwrap()
                {
                    break;
                }
            }
        }
    }
}

pub fn logout(toml_path: &str, old_config: Config, account: Option<String>) {
    let mut config = Config {
        accounts: Vec::new(),
    };

    if Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Are you sure you want to logout?")
        .default(true)
        .show_default(true)
        .wait_for_newline(true)
        .interact()
        .unwrap()
    {
        let mut found = false;
        if let Some(account) = account {
            if let Ok(id) = account.parse::<u32>() {
                for acc in old_config.accounts {
                    if acc.id == id {
                        found = true;
                        let entry = Entry::new(APP_NAME, &id.to_string()).unwrap_or_else(|err| {
                            eprintln!("Unexpected Error: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });

                        entry.delete_credential().unwrap_or_else(|err| {
                            eprintln!("Error when deleting credential: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });

                        if acc.oidc.is_some() {
                            let client_id_entry = Entry::new(APP_NAME, &format!("{}.id", &id))
                                .unwrap_or_else(|err| {
                                    eprintln!("Unexpected Error: {}", err);
                                    keyring_core::unset_default_store();
                                    process::exit(1);
                                });

                            client_id_entry.delete_credential().unwrap_or_else(|err| {
                                eprintln!("Failed to get Client Id: {}", err);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            });

                            let refresh_token_entry =
                                Entry::new(APP_NAME, &format!("{}.refresh_token", &id))
                                    .unwrap_or_else(|err| {
                                        eprintln!("Unexpected Error: {}", err);
                                        keyring_core::unset_default_store();
                                        process::exit(1);
                                    });

                            if refresh_token_entry.delete_credential().is_ok() {};
                        }
                    } else {
                        config.accounts.push(acc);
                    }
                }
            } else {
                for acc in old_config.accounts {
                    if acc.email == account {
                        found = true;
                        let entry =
                            Entry::new(APP_NAME, &acc.id.to_string()).unwrap_or_else(|err| {
                                eprintln!("Unexpected Error: {}", err);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            });

                        entry.delete_credential().unwrap_or_else(|err| {
                            eprintln!("Error when deleting credential: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });

                        if acc.oidc.is_some() {
                            let client_id_entry = Entry::new(APP_NAME, &format!("{}.id", &acc.id))
                                .unwrap_or_else(|err| {
                                    eprintln!("Unexpected Error: {}", err);
                                    keyring_core::unset_default_store();
                                    process::exit(1);
                                });

                            client_id_entry.delete_credential().unwrap_or_else(|err| {
                                eprintln!("Failed to delete Client Id: {}", err);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            });

                            let refresh_token_entry =
                                Entry::new(APP_NAME, &format!("{}.refresh_token", &acc.id))
                                    .unwrap_or_else(|err| {
                                        eprintln!("Unexpected Error: {}", err);
                                        keyring_core::unset_default_store();
                                        process::exit(1);
                                    });

                            if refresh_token_entry.delete_credential().is_ok() {};
                        }
                    } else {
                        config.accounts.push(acc);
                    }
                }
            }
        } else {
            // shift active to some other
            // or easier:
            // user has to switch manually
            for acc in old_config.accounts {
                if acc.active {
                    found = true;
                    let entry = Entry::new(APP_NAME, &acc.id.to_string()).unwrap_or_else(|err| {
                        eprintln!("Unexpected Error: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                    entry.delete_credential().unwrap_or_else(|err| {
                        eprintln!("Error when deleting credential: {}", err);
                        keyring_core::unset_default_store();
                        process::exit(1);
                    });

                    if acc.oidc.is_some() {
                        let client_id_entry = Entry::new(APP_NAME, &format!("{}.id", &acc.id))
                            .unwrap_or_else(|err| {
                                eprintln!("Unexpected Error: {}", err);
                                keyring_core::unset_default_store();
                                process::exit(1);
                            });

                        client_id_entry.delete_credential().unwrap_or_else(|err| {
                            eprintln!("Failed to get Client Id: {}", err);
                            keyring_core::unset_default_store();
                            process::exit(1);
                        });

                        let refresh_token_entry =
                            Entry::new(APP_NAME, &format!("{}.refresh_token", &acc.id))
                                .unwrap_or_else(|err| {
                                    eprintln!("Unexpected Error: {}", err);
                                    keyring_core::unset_default_store();
                                    process::exit(1);
                                });

                        if refresh_token_entry.delete_credential().is_ok() {};
                    }
                } else {
                    config.accounts.push(acc);
                }
            }
            // shift active to first account (because what else should i do, predict what account the
            // user wants?)
            if !config.accounts.is_empty() {
                config.accounts[0].active = true;
            }

            if !found {
                println!("No account is currently active");
            }
        }

        if found {
            let toml_output = toml::to_string(&config).unwrap_or_else(|err| {
                eprintln!("Unexpected Error: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

            fs::write(toml_path, toml_output).unwrap_or_else(|err| {
                eprintln!("Error when saving new config: {}", err);
                keyring_core::unset_default_store();
                process::exit(1);
            });

            println!("Logout successful");
        }
    }
}
